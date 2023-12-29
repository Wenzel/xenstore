use libloading::Error;
use std::os::raw::{c_char, c_int, c_uint, c_ulong, c_void};
use xenstore_sys::{xs_handle, xs_transaction_t};

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
// xs_watch
type FnWatch = fn(h: *mut xs_handle, path: *const c_char, token: *const c_char) -> bool;
// xs_fileno
type FnFileno = fn(h: *mut xs_handle) -> c_int;
// xs_check_watch
type FnCheckWatch = fn(h: *mut xs_handle) -> *mut *mut c_char;
// xs_read_watch
type FnReadWatch = fn(h: *mut xs_handle, num: *mut c_uint) -> *mut *mut c_char;
// xs_unwatch
type FnUnwatch = fn(h: *mut xs_handle, path: *const c_char, token: *const c_char) -> bool;

#[cfg(not(feature = "static"))]
mod dynamic {
    use {
        super::*,
        libloading::os::unix::Symbol as RawSymbol,
        libloading::{Library, Symbol},
    };

    const LIBXENSTORE_SONAME_LIST: [&str; 2] = ["3.0", "4"];
    const LIBXENSTORE_BASENAME: &str = "libxenstore.so";

    #[derive(Debug)]
    pub struct LibXenStore {
        _lib: Library,
        pub open: RawSymbol<FnOpen>,
        pub close: RawSymbol<FnClose>,
        pub directory: RawSymbol<FnDirectory>,
        pub read: RawSymbol<FnRead>,
        pub rm: RawSymbol<FnRm>,
        pub write: RawSymbol<FnWrite>,

        pub watch: RawSymbol<FnWatch>,
        pub fileno: RawSymbol<FnFileno>,
        pub check_watch: RawSymbol<FnCheckWatch>,
        pub read_watch: RawSymbol<FnReadWatch>,
        pub unwatch: RawSymbol<FnUnwatch>,
    }

    impl LibXenStore {
        /// Loads the libxenstore.so library dynamically, by trying multiple SONAMES
        /// On failure it returns the last load error
        pub unsafe fn new() -> Result<Self, Error> {
            use log::info;
            use std::ffi::OsString;

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

                    let watch_sym: Symbol<FnWatch> = lib.get(b"xs_watch\0")?;
                    let watch = watch_sym.into_raw();

                    let fileno_sym: Symbol<FnFileno> = lib.get(b"xs_fileno\0")?;
                    let fileno = fileno_sym.into_raw();

                    let check_watch_sym: Symbol<FnCheckWatch> = lib.get(b"xs_check_watch\0")?;
                    let check_watch = check_watch_sym.into_raw();

                    let read_watch_sym: Symbol<FnReadWatch> = lib.get(b"xs_read_watch\0")?;
                    let read_watch = read_watch_sym.into_raw();

                    let unwatch_sym: Symbol<FnUnwatch> = lib.get(b"xs_unwatch\0")?;
                    let unwatch = unwatch_sym.into_raw();

                    return Ok(LibXenStore {
                        _lib: lib,
                        open,
                        close,
                        directory,
                        read,
                        rm,
                        write,
                        watch,
                        fileno,
                        check_watch,
                        read_watch,
                        unwatch,
                    });
                }
            }
            Err(last_load_error.unwrap())
        }
    }
}

#[cfg(feature = "static")]
mod statik {
    use super::*;

    #[derive(Debug)]
    pub struct LibXenStore {
        pub open: FnOpen,
        pub close: FnClose,
        pub directory: FnDirectory,
        pub read: FnRead,
        pub rm: FnRm,
        pub write: FnWrite,

        pub watch: FnWatch,
        pub fileno: FnFileno,
        pub check_watch: FnCheckWatch,
        pub read_watch: FnReadWatch,
        pub unwatch: FnUnwatch,
    }

    impl LibXenStore {
        pub unsafe fn new() -> Result<Self, Error> {
            use xenstore_sys::{
                xs_check_watch, xs_close, xs_directory, xs_fileno, xs_open, xs_read, xs_read_watch,
                xs_rm, xs_unwatch, xs_watch, xs_write,
            };

            // write safe Rust wrappers to unsafe extern "C" functions
            fn safe_open(flags: c_ulong) -> *mut xs_handle {
                unsafe { xs_open(flags) }
            }

            fn safe_close(xsh: *mut xs_handle) {
                unsafe { xs_close(xsh) }
            }

            fn safe_directory(
                h: *mut xs_handle,
                t: xs_transaction_t,
                path: *const c_char,
                num: *mut c_uint,
            ) -> *mut *mut c_char {
                unsafe { xs_directory(h, t, path, num) }
            }

            fn safe_read(
                h: *mut xs_handle,
                t: xs_transaction_t,
                path: *const c_char,
                len: *mut c_uint,
            ) -> *mut c_void {
                unsafe { xs_read(h, t, path, len) }
            }

            fn safe_rm(h: *mut xs_handle, t: xs_transaction_t, path: *const c_char) -> bool {
                unsafe { xs_rm(h, t, path) }
            }

            fn safe_write(
                h: *mut xs_handle,
                t: xs_transaction_t,
                path: *const c_char,
                data: *const c_void,
                len: c_uint,
            ) -> bool {
                unsafe { xs_write(h, t, path, data, len) }
            }

            fn safe_watch(h: *mut xs_handle, path: *const c_char, token: *const c_char) -> bool {
                unsafe { xs_watch(h, path, token) }
            }

            fn safe_fileno(h: *mut xs_handle) -> c_int {
                unsafe { xs_fileno(h) }
            }

            fn safe_check_watch(h: *mut xs_handle) -> *mut *mut c_char {
                unsafe { xs_check_watch(h) }
            }

            fn safe_read_watch(h: *mut xs_handle, num: *mut c_uint) -> *mut *mut c_char {
                unsafe { xs_read_watch(h, num) }
            }

            fn safe_unwatch(h: *mut xs_handle, path: *const c_char, token: *const c_char) -> bool {
                unsafe { xs_unwatch(h, path, token) }
            }

            Ok(LibXenStore {
                open: safe_open,
                close: safe_close,
                directory: safe_directory,
                read: safe_read,
                rm: safe_rm,
                write: safe_write,
                watch: safe_watch,
                fileno: safe_fileno,
                check_watch: safe_check_watch,
                read_watch: safe_read_watch,
                unwatch: safe_unwatch,
            })
        }
    }
}

#[cfg(not(feature = "static"))]
pub use dynamic::LibXenStore;
#[cfg(feature = "static")]
pub use statik::LibXenStore;
