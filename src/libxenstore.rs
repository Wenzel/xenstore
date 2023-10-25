use std::os::raw::{c_char, c_uint, c_ulong, c_void};

use std::ffi::OsString;
use xenstore_sys::{xs_handle, xs_transaction_t};

use libloading::os::unix::Symbol as RawSymbol;
use libloading::{Error, Library, Symbol};
use log::info;

const LIBXENSTORE_SONAME_LIST: [&str; 2] = ["3.0", "4"];
const LIBXENSTORE_BASENAME: &str = "libxenstore.so.";
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
type FnRm = fn(h: *mut xs_handle, t: xs_transaction_t, path: *const c_char) -> bool;
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
    _lib: Library,
    pub open: RawSymbol<FnOpen>,
    pub close: RawSymbol<FnClose>,
    pub directory: RawSymbol<FnDirectory>,
    pub read: RawSymbol<FnRead>,
    pub rm: RawSymbol<FnRm>,
    pub write: RawSymbol<FnWrite>,
}

impl LibXenStore {
    /// Loads the libxenstore.so library dynamically, by trying multiple SONAMES
    /// On failure it returns the last load error
    pub unsafe fn new() -> Result<Self, Error> {
        let mut last_load_error: Option<Error> = None;
        for soname in LIBXENSTORE_SONAME_LIST {
            let lib_filename = format!("{}.{}", LIBXENSTORE_BASENAME, soname);
            info!("Loading {}", lib_filename);
            let result = Library::new::<OsString>(lib_filename.clone().into());
            if let Err(err) = result {
                info!("Failed to load {}", lib_filename);
                last_load_error = Some(err);
                continue;
            } else {
                let lib = result.unwrap();
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

                return Ok(LibXenStore {
                    _lib: lib,
                    open,
                    close,
                    directory,
                    read,
                    rm,
                    write,
                });
            }
        }
        Err(last_load_error.unwrap())
    }
}
