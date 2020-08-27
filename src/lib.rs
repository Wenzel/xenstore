mod libxenstore;

use std::ffi::{CStr, CString};
use std::io::Error;
use std::os::raw::c_char;
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

pub trait XsIntrospectable: std::fmt::Debug {
    fn init(&mut self, open_type: XsOpenFlags) -> Result<(), Error>;
    fn directory(&self, transaction: XBTransaction, path: String) -> Vec<String>;
    fn read(&self, transaction: XBTransaction, path: String) -> String;
    fn close(&mut self);
}

pub fn create_xen_store() -> Xs {
    Xs::new(unsafe { LibXenStore::new() })
}

impl Xs {
    fn new(libxenstore: LibXenStore) -> Xs {
        Xs {
            handle: null_mut(),
            libxenstore,
        }
    }
}

impl XsIntrospectable for Xs {
    fn init(&mut self, open_type: XsOpenFlags) -> Result<(), Error> {
        let xs_handle = (self.libxenstore.open)(open_type as u64);
        if xs_handle == null_mut() {
            return Err(Error::last_os_error());
        }
        self.handle = xs_handle;
        Ok(())
    }

    fn directory(&self, transaction: XBTransaction, path: String) -> Vec<String> {
        let mut num = 0;
        let c_path = CString::new(path).unwrap();
        let mut dir: Vec<String> = Vec::new();
        let trans_value = transaction.to_u32().expect("Invalid transaction value");
        let res = (self.libxenstore.directory)(self.handle, trans_value, c_path.as_ptr(), &mut num);
        unsafe {
            let array: &[*mut c_char] = slice::from_raw_parts_mut(res, num as usize);
            for x in array {
                dir.push(CStr::from_ptr(*x).to_string_lossy().into_owned());
            }
            // TODO: free array
        };
        dir
    }

    fn read(&self, transaction: XBTransaction, path: String) -> String {
        let mut len = 0;
        let c_path = CString::new(path).unwrap();
        let trans_value = transaction.to_u32().expect("Invalid transaction value");
        let res = (self.libxenstore.read)(self.handle, trans_value, c_path.as_ptr(), &mut len);
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
