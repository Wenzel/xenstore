mod libxenstore;

use std::ffi::{CStr, CString};
use std::io::Error;
use std::os::raw::{c_char, c_uint};
use std::ptr::null_mut;
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
    pub fn new(open_type: XsOpenFlags) -> Result<Self, Error> {
        let libxenstore = unsafe { LibXenStore::new() };
        let xs_handle = (libxenstore.open)(open_type as u64);
        if xs_handle == null_mut() {
            return Err(Error::last_os_error());
        }
        return Ok(Xs {
            handle: xs_handle,
            libxenstore,
        });
    }

    pub fn directory(&self, transaction: XBTransaction, path: String) -> Vec<String> {
        let mut num = 0;
        let num_ptr: *mut c_uint = &mut num;
        let c_path = CString::new(path).unwrap();
        let mut dir: Vec<String> = Vec::new();
        let trans_value = transaction.to_u32().expect("Invalid transaction value");
        let res =
            (self.libxenstore.directory)(self.handle, trans_value, c_path.as_ptr(), num_ptr);
        unsafe {
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
        let trans_value = transaction.to_u32().expect("Invalid transaction value");
        let res =
            (self.libxenstore.read)(self.handle, trans_value, c_path.as_ptr(), len_ptr);
        unsafe {
            CStr::from_ptr(res as *mut c_char)
                .to_string_lossy()
                .into_owned()
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
