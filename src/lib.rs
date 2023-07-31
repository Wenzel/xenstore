mod libxenstore;

use std::convert::TryFrom;
use std::error::Error;
use std::ffi::{c_void, CStr, CString};
use std::io::{self, Error as IoError};
use std::num::NonZeroU32;
use std::os::fd::RawFd;
use std::os::raw::{c_char, c_uint};
use std::pin::Pin;
use std::slice;
use std::task::{Context, Poll};

use futures::Stream;
use tokio::io::unix::AsyncFd;
use tokio::io::Interest;

use libxenstore::LibXenStore;

pub struct XBTransaction(NonZeroU32);

#[repr(u32)]
pub enum XsOpenFlags {
    ReadOnly = xenstore_sys::XS_OPEN_READONLY,
    SocketOnly = xenstore_sys::XS_OPEN_SOCKETONLY,
}

#[derive(Debug)]
pub struct Xs {
    handle: *mut xenstore_sys::xs_handle,
    libxenstore: LibXenStore,
}

unsafe impl Send for Xs {}
unsafe impl Sync for Xs {}

#[derive(Debug)]
pub struct XsWatchEntry {
    pub path: String,
    pub token: String,
}

#[cfg(feature = "async_watch")]
#[derive(Debug)]
pub struct XsStream<'a> {
    xs: &'a Xs,

    // Keep the instance of AsyncFd to make sure it will be able to wake up the task on readiness.
    fd: Option<AsyncFd<i32>>,
    current_fd: Option<i32>,
}

unsafe impl Send for XsStream<'_> {}
unsafe impl Sync for XsStream<'_> {}

#[cfg(feature = "async_watch")]
impl Stream for XsStream<'_> {
    type Item = XsWatchEntry;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.xs.check_watch() {
            // Entry available, return it.
            Ok(Some(entry)) => Poll::Ready(Some(entry)),
            // No entry, poll xs_fileno.
            Ok(None) => {
                let new_fd = (self.xs.libxenstore.fileno)(self.xs.handle);

                // Prevent having the fd registered twice, as it makes AsyncFd::with_interest failing.
                // Only update it if it has changed.
                if new_fd != self.current_fd.unwrap_or(-1) {
                    let Ok(fd) = AsyncFd::with_interest(RawFd::from(new_fd), Interest::READABLE)
                    else {
                        // Unable to use fd
                        return Poll::Ready(None);
                    };

                    self.current_fd.replace(new_fd);
                    self.fd.replace(fd);
                }

                // Poll file descriptor
                return match self.fd.as_mut().unwrap().poll_read_ready(cx) {
                    Poll::Ready(Ok(mut guard)) => {
                        // Latest read has no data available, clear ready flag and retry.
                        guard.clear_ready();
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                    // Poll failure, report end of stream.
                    Poll::Ready(Err(_)) => Poll::Ready(None),
                    Poll::Pending => Poll::Pending,
                };
            }
            // Check failed, report end of stream.
            Err(_) => Poll::Ready(None),
        }
    }
}

impl Xs {
    pub fn new(open_type: XsOpenFlags) -> Result<Self, Box<dyn Error>> {
        let libxenstore = unsafe { LibXenStore::new()? };
        let xs_handle = (libxenstore.open)(open_type as u64);
        if xs_handle.is_null() {
            Err(Box::new(IoError::last_os_error()))
        } else {
            Ok(Xs {
                handle: xs_handle,
                libxenstore,
            })
        }
    }

    pub fn directory(
        &self,
        transaction: Option<XBTransaction>,
        path: &str,
    ) -> Result<Vec<String>, IoError> {
        let mut num = 0;
        let c_path = CString::new(path).unwrap();
        let trans_value = self.get_trans_value(transaction);
        let res = (self.libxenstore.directory)(self.handle, trans_value, c_path.as_ptr(), &mut num);
        if res.is_null() {
            Err(IoError::last_os_error())
        } else {
            let mut dir: Vec<String> = Vec::new();
            unsafe {
                let array: &[*mut c_char] = slice::from_raw_parts_mut(res, num as usize);
                for x in array {
                    dir.push(CStr::from_ptr(*x).to_string_lossy().into_owned());
                }
                libc::free(res as *mut c_void);
            };
            Ok(dir)
        }
    }

    pub fn read(&self, transaction: Option<XBTransaction>, path: &str) -> Result<String, IoError> {
        let mut len = 0;
        let c_path = CString::new(path).unwrap();
        let trans_value = self.get_trans_value(transaction);
        let res = (self.libxenstore.read)(self.handle, trans_value, c_path.as_ptr(), &mut len);
        if res.is_null() {
            Err(IoError::last_os_error())
        } else {
            unsafe {
                let res_string = CStr::from_ptr(res as *mut c_char)
                    .to_string_lossy()
                    .into_owned();
                libc::free(res);
                Ok(res_string)
            }
        }
    }

    pub fn write(
        &self,
        transaction: Option<XBTransaction>,
        path: &str,
        data: &str,
    ) -> Result<(), IoError> {
        let char_data = data.as_bytes();
        let len: c_uint = c_uint::try_from(char_data.len()).expect("Too much data");
        let c_path = CString::new(path).unwrap();
        let trans_value = self.get_trans_value(transaction);
        let res = (self.libxenstore.write)(
            self.handle,
            trans_value,
            c_path.as_ptr(),
            char_data.as_ptr() as *const c_void,
            len,
        );
        if res {
            Ok(())
        } else {
            Err(IoError::last_os_error())
        }
    }

    pub fn rm(&self, transaction: Option<XBTransaction>, path: &str) -> Result<(), IoError> {
        let c_path = CString::new(path).unwrap();
        let trans_value = self.get_trans_value(transaction);
        let res = (self.libxenstore.rm)(self.handle, trans_value, c_path.as_ptr());
        if res {
            Ok(())
        } else {
            Err(IoError::last_os_error())
        }
    }

    pub fn watch(&self, path: &str, token: &str) -> Result<(), IoError> {
        let c_path = CString::new(path).unwrap();
        let c_token = CString::new(token).unwrap();

        let res = (self.libxenstore.watch)(self.handle, c_path.as_ptr(), c_token.as_ptr());

        if res {
            Ok(())
        } else {
            Err(IoError::last_os_error())
        }
    }

    pub fn read_watch(&self) -> Result<Vec<XsWatchEntry>, IoError> {
        let mut count = 0u32;
        let res = (self.libxenstore.read_watch)(self.handle, &mut count);

        if res.is_null() {
            return Err(IoError::last_os_error());
        }

        // Each entry is two strings.
        let entries_raw = unsafe { slice::from_raw_parts(res, count as usize) };

        let entries = entries_raw
            .chunks_exact(2)
            .map(|slice| unsafe {
                XsWatchEntry {
                    path: CStr::from_ptr(slice[0]).to_string_lossy().into_owned(),
                    token: CStr::from_ptr(slice[1]).to_string_lossy().into_owned(),
                }
            })
            .collect();

        unsafe {
            libc::free(res as _);
        }

        Ok(entries)
    }

    pub fn check_watch(&self) -> Result<Option<XsWatchEntry>, IoError> {
        let res = (self.libxenstore.check_watch)(self.handle);

        if res.is_null() {
            return if matches!(io::Error::last_os_error().kind(), io::ErrorKind::WouldBlock) {
                Ok(None)
            } else {
                Err(io::Error::last_os_error())
            };
        }

        let entry = unsafe {
            let slice = slice::from_raw_parts(res, 2);

            XsWatchEntry {
                path: CStr::from_ptr(slice[0]).to_string_lossy().into_owned(),
                token: CStr::from_ptr(slice[1]).to_string_lossy().into_owned(),
            }
        };

        unsafe {
            libc::free(res as _);
        }

        Ok(Some(entry))
    }

    pub fn unwatch(&self, path: &str, token: &str) -> Result<(), IoError> {
        let c_path = CString::new(path).unwrap();
        let c_token = CString::new(token).unwrap();

        let res = (self.libxenstore.unwatch)(self.handle, c_path.as_ptr(), c_token.as_ptr());

        if res {
            Ok(())
        } else {
            Err(IoError::last_os_error())
        }
    }

    #[cfg(feature = "async_watch")]
    pub fn get_stream(&self) -> Result<XsStream, IoError> {
        Ok(XsStream {
            xs: self,
            fd: None,
            current_fd: None,
        })
    }

    /// Helper to get the transaction value from Option<XBTransaction>
    /// either returns xenstore_sys::XBT_NULL if None
    /// or the non-zero u32 value otherwise
    fn get_trans_value(&self, trans: Option<XBTransaction>) -> u32 {
        trans.map(|v| v.0.get()).unwrap_or(xenstore_sys::XBT_NULL)
    }

    fn close(&mut self) {
        (self.libxenstore.close)(self.handle);
    }
}

impl Drop for Xs {
    fn drop(&mut self) {
        self.close();
    }
}
