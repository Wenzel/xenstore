use std::os::raw::{c_char, c_uint, c_ulong, c_void};

use xenstore_sys::{xs_handle, xs_transaction_t};

use libloading::os::unix::Symbol as RawSymbol;
use libloading::{library_filename, Error, Library, Symbol};
use log::info;

const LIBXENSTORE_BASENAME: &str = "xenstore";
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
// xs_rm
type FnRm = fn(
    h: *mut xs_handle,
    t: xs_transaction_t,
    path: *const c_char,
) -> bool;
// xs_write
type FnWrite = fn(
    h: *mut xs_handle,
    t: xs_transaction_t,
    path: *const c_char,
    data: *const c_void,
    len: c_uint,
) -> bool;

#[derive(Debug)]
pub struct LibXenStore {
    lib: Library,
    pub open: RawSymbol<FnOpen>,
    pub close: RawSymbol<FnClose>,
    pub directory: RawSymbol<FnDirectory>,
    pub read: RawSymbol<FnRead>,
    pub rm: RawSymbol<FnRm>,
    pub write: RawSymbol<FnWrite>,
}

impl LibXenStore {
    pub unsafe fn new() -> Result<Self, Error> {
        let lib_filename = library_filename(LIBXENSTORE_BASENAME);
        info!("Loading {}", lib_filename.to_str().unwrap());
        let lib = Library::new(lib_filename)?;
        // load symbols
        let open_sym: Symbol<FnOpen> = lib.get(b"xs_open\0")?;
        let open = open_sym.into_raw();

        let close_sym: Symbol<FnClose> = lib.get(b"xs_close\0")?;
        let close = close_sym.into_raw();

        let directory_sym: Symbol<FnDirectory> = lib.get(b"xs_directory\0")?;
        let directory = directory_sym.into_raw();

        let read_sym: Symbol<FnRead> = lib.get(b"xs_read\0")?;
        let read = read_sym.into_raw();

        let rm_sym: Symbol<FnRm> = lib.get(b"xs_rm\0")?;
        let rm = rm_sym.into_raw();

        let write_sym: Symbol<FnWrite> = lib.get(b"xs_write\0")?;
        let write = write_sym.into_raw();

        Ok(LibXenStore {
            lib,
            open,
            close,
            directory,
            read,
            rm,
            write,
        })
    }
}
