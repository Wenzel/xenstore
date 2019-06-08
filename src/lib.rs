extern crate xenstore_sys;
use std::io::Error;
use std::ptr::{null_mut};

pub struct Xs {
    handle: *mut xenstore_sys::xs_handle,
}

impl Xs {

    pub fn new() -> Result<Self,Error> {
        let xs_handle = unsafe {
            let result = xenstore_sys::xs_open(xenstore_sys::XS_OPEN_READONLY.into());
            result
        };
        if xs_handle == null_mut() {
            return Err(Error::last_os_error());
        }
        return Ok(Xs {
            handle: xs_handle,
        });
    }

    pub fn close(&mut self) {
        unsafe {
            xenstore_sys::xs_close(self.handle);
        };
    }
}

