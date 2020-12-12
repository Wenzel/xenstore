use std::os::raw::{c_char, c_uint, c_ulong, c_void};

use xenstore_sys::{xs_handle, xs_transaction_t};

use libloading::os::unix::Symbol as RawSymbol;
use libloading::{Library, Symbol};
use log::info;

const LIBXENSTORE_FILENAME: &str = "libxenstore.so";
// xs_open
type FnOpen = fn(flags: c_ulong) -> *mut xs_handle;
// xs_close
type FnClose = fn(xsh: *mut xs_handle);
// xs_directory
type FnDirectory = fn(
    h: *mut xs_handle,
    t: xs_transaction_t,
    path: *const c_char,
    num: *mut c_uint,
) -> *mut *mut c_char;
// xs_read
type FnRead = fn(
    h: *mut xs_handle,
    t: xs_transaction_t,
    path: *const c_char,
    len: *mut c_uint,
) -> *mut c_void;

#[derive(Debug)]
pub struct LibXenStore {
    lib: Library,
    pub open: RawSymbol<FnOpen>,
    pub close: RawSymbol<FnClose>,
    pub directory: RawSymbol<FnDirectory>,
    pub read: RawSymbol<FnRead>,
}

impl LibXenStore {
    pub unsafe fn new() -> Self {
        info!("Loading {}", LIBXENSTORE_FILENAME);
        let lib = Library::new(LIBXENSTORE_FILENAME).unwrap();
        // load symbols
        let open_sym: Symbol<FnOpen> = lib.get(b"xs_open\0").unwrap();
        let open = open_sym.into_raw();

        let close_sym: Symbol<FnClose> = lib.get(b"xs_close\0").unwrap();
        let close = close_sym.into_raw();

        let directory_sym: Symbol<FnDirectory> = lib.get(b"xs_directory\0").unwrap();
        let directory = directory_sym.into_raw();

        let read_sym: Symbol<FnRead> = lib.get(b"xs_read\0").unwrap();
        let read = read_sym.into_raw();

        LibXenStore {
            lib,
            open,
            close,
            directory,
            read,
        }
    }
}
