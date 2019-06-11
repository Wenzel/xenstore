extern crate xenstore_sys;
use std::io::Error;
use std::ptr::{null_mut};
use std::os::raw::{c_uint, c_char};
use std::ffi::{CString, CStr};
use std::slice;

#[repr(u32)]
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
}

impl Xs {

    pub fn new(open_type: XsOpenFlags) -> Result<Self,Error> {
        let xs_handle = unsafe {
            xenstore_sys::xs_open(open_type as u64)
        };
        if xs_handle == null_mut() {
            return Err(Error::last_os_error());
        }
        return Ok(Xs {
            handle: xs_handle,
        });
    }

    pub fn directory(&self, transaction: XBTransaction, path: String) -> Vec<String> {
        let mut num = 0;
        let num_ptr: *mut c_uint = &mut num;
        let c_path = CString::new(path).unwrap();
        let mut dir: Vec<String> = Vec::new();
        unsafe {
            let res = xenstore_sys::xs_directory(self.handle, transaction as u32, c_path.as_ptr(), num_ptr);
            let array: &[*mut c_char] = slice::from_raw_parts_mut(res, num as usize);
            for x in array {
                dir.push(CStr::from_ptr(*x).to_string_lossy().into_owned());
            }
            // TODO: free array
        };
        dir
    }

    pub fn read(&self, transaction: XBTransaction, path: String) -> String {
        let mut len = 0;
        let len_ptr: *mut c_uint = &mut len;
        let c_path = CString::new(path).unwrap();
        unsafe {
            let res = xenstore_sys::xs_read(self.handle, transaction as u32, c_path.as_ptr(), len_ptr);
            CStr::from_ptr(res as *mut c_char).to_string_lossy().into_owned()
        }
    }

    pub fn close(&mut self) {
        unsafe {
            xenstore_sys::xs_close(self.handle);
        };
    }
}

impl Drop for Xs {
    fn drop(&mut self) {
        self.close();
    }
}

