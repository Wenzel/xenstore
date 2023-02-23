mod libxenstore;

use std::convert::TryFrom;
use std::error::Error;
use std::ffi::{c_void, CStr, CString};
use std::io::Error as IoError;
use std::os::raw::{c_char, c_uint};
use std::slice;

#[macro_use]
extern crate enum_primitive_derive;
use num_traits::ToPrimitive;

use libxenstore::LibXenStore;

#[repr(u32)]
#[derive(Primitive)]
pub enum XBTransaction {
    Null = xenstore_sys::XBT_NULL,
}

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
        transaction: XBTransaction,
        path: &str,
    ) -> Result<Vec<String>, IoError> {
        let mut num = 0;
        let c_path = CString::new(path).unwrap();
        let trans_value = transaction.to_u32().expect("Invalid transaction value");
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

    pub fn read(&self, transaction: XBTransaction, path: &str) -> Result<String, IoError> {
        let mut len = 0;
        let c_path = CString::new(path).unwrap();
        let trans_value = transaction.to_u32().expect("Invalid transaction value");
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

    pub fn write(&self, transaction: XBTransaction, path: &str, data: &str
    ) -> Result<(), IoError> {
        let char_data = data.as_bytes();
        let len: c_uint = c_uint::try_from(char_data.len())
            .expect("Too much data");
        let c_path = CString::new(path).unwrap();
        let trans_value = transaction.to_u32().expect("Invalid transaction value");
        let res = (self.libxenstore.write)(self.handle, trans_value, c_path.as_ptr(),
                                           char_data.as_ptr() as *const c_void, len);
        if res {
            Ok(())
        } else {
            Err(IoError::last_os_error())
        }
    }

    pub fn rm(&self, transaction: XBTransaction, path: &str) -> Result<(), IoError> {
        let c_path = CString::new(path).unwrap();
        let trans_value = transaction.to_u32().expect("Invalid transaction value");
        let res = (self.libxenstore.rm)(self.handle, trans_value, c_path.as_ptr());
        if res {
            Ok(())
        } else {
            Err(IoError::last_os_error())
        }
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
