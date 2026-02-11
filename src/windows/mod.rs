//! Xenstore Windows implementation.
//! Rely on xeniface driver.
//!
mod device;
mod utils;

#[cfg(feature = "async-smol")]
pub mod smol;

use std::{
    ffi::{c_void, CString},
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
};

use device::{DeviceInfoList, GUID_INTERFACE_XENIFACE};
use utils::{make_payload, parse_nul_list, parse_nul_string};

use log::{debug, warn};
use windows::{
    core::{Result, PCWSTR},
    Win32::{
        Foundation::{ERROR_NOT_FOUND, GENERIC_READ, GENERIC_WRITE, HANDLE},
        Storage::FileSystem::{
            CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
            OPEN_EXISTING,
        },
        System::{Threading::CreateEventW, IO::DeviceIoControl},
    },
};

use crate::Xs;

// Well, there is no CTL_CODE in the windows crate so we need to add it ourselves.
// Taken from https://docs.rs/winapi/latest/src/winapi/um/winioctl.rs.html#146-153
fn ctl_code(device_type: u32, function: u32, method: u32, access: u32) -> u32 {
    (device_type << 16) | (access << 14) | (function << 2) | method
}

// Same for some ioctl constants
const METHOD_BUFFERED: u32 = 0;
const FILE_ANY_ACCESS: u32 = 0;
const FILE_DEVICE_UNKNOWN: u32 = 0x22;

/// Xenstore Windows implementation.
#[derive(Debug)]
pub struct XsWindows(OwnedHandle);

impl XsWindows {
    /// Try to open Xenstore interface.
    ///
    /// Uses the first working xeniface device (GUID = b2cfb085-aa5e-47e1-8bf7-9793f3154565).
    pub fn new() -> Result<Self> {
        // Try all devices with XENIFACE class.
        let dev_list = DeviceInfoList::new(GUID_INTERFACE_XENIFACE).unwrap();

        for raw_wpath in dev_list.iter() {
            let wpath = PCWSTR::from_raw(raw_wpath.as_ptr());
            debug!("Trying {}", unsafe { wpath.display() });

            match unsafe {
                CreateFileW(
                    wpath,
                    (GENERIC_READ | GENERIC_WRITE).0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAGS_AND_ATTRIBUTES::default(),
                    None,
                )
            } {
                Ok(file) => {
                    debug!("Got {file:?}");
                    return Ok(XsWindows(unsafe { OwnedHandle::from_raw_handle(file.0) }));
                }
                Err(e) => {
                    warn!("Unable to open {} ({e})", unsafe { wpath.display() })
                }
            }
        }

        return Err(ERROR_NOT_FOUND.into());
    }

    fn make_ioctl(
        &self,
        control_code: u32,
        in_buffer: &[u8],
        out_buffer: Option<&mut [u8]>,
    ) -> Result<u32> {
        let mut len = 0;
        let out_buffer_len = out_buffer.as_ref().map_or(0, |s| s.len());

        unsafe {
            DeviceIoControl(
                HANDLE(self.0.as_raw_handle()),
                control_code,
                Some(in_buffer.as_ptr().cast()),
                in_buffer.len() as u32,
                out_buffer.map(|r| r.as_mut_ptr().cast()),
                out_buffer_len as u32,
                Some(&mut len),
                None,
            )?;
        }

        Ok(len)
    }
}

impl Xs for XsWindows {
    fn directory(&self, path: &str) -> io::Result<Vec<Box<str>>> {
        let in_buffer = make_payload(&[path]);
        let mut out_buffer = vec![0u8; 4096];

        /* Enumerate all immediate child keys of a XenStore key
         *  Input: NUL-terminated CHAR array containing the requested key's path
         *  Output: List of NUL-terminated CHAR arrays containing the child key names,
         *          followed by a NUL CHAR
         *  #define IOCTL_XENIFACE_STORE_DIRECTORY \
         *      CTL_CODE(FILE_DEVICE_UNKNOWN, 0x802, METHOD_BUFFERED, FILE_ANY_ACCESS)
         */
        let len = self.make_ioctl(
            ctl_code(FILE_DEVICE_UNKNOWN, 0x802, METHOD_BUFFERED, FILE_ANY_ACCESS),
            &in_buffer,
            Some(&mut out_buffer),
        )?;
        out_buffer.truncate(len as usize);

        Ok(parse_nul_list(&out_buffer)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
            .iter()
            .map(|s| s.to_string().into_boxed_str())
            .collect())
    }

    fn read(&self, path: &str) -> io::Result<Box<str>> {
        let in_buffer = make_payload(&[path]);
        let mut out_buffer = vec![0u8; 4096];

        /* Read a value from XenStore
         *  Input: NUL-terminated CHAR array containing the requested key's path
         *  Output: NUL-terminated CHAR array containing the requested key's value
         *  #define IOCTL_XENIFACE_STORE_READ \
         *      CTL_CODE(FILE_DEVICE_UNKNOWN, 0x800, METHOD_BUFFERED, FILE_ANY_ACCESS)
         */
        let len = self.make_ioctl(
            ctl_code(FILE_DEVICE_UNKNOWN, 0x800, METHOD_BUFFERED, FILE_ANY_ACCESS),
            &in_buffer,
            Some(&mut out_buffer),
        )?;
        out_buffer.truncate(len as usize);

        Ok(parse_nul_string(&out_buffer)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
            .unwrap_or_default()
            .to_string()
            .into_boxed_str())
    }

    fn write(&self, path: &str, data: &str) -> io::Result<()> {
        let in_buffer = make_payload(&[path, data]);

        /* Write a value to XenStore
         *  Input: NUL-terminated CHAR array containing the requested key's path,
         *         NUL-terminated CHAR array containing the key's value, final NUL terminator
         *  Output: None
         * #define IOCTL_XENIFACE_STORE_WRITE \
         *     CTL_CODE(FILE_DEVICE_UNKNOWN, 0x801, METHOD_BUFFERED, FILE_ANY_ACCESS)
         */
        self.make_ioctl(
            ctl_code(FILE_DEVICE_UNKNOWN, 0x801, METHOD_BUFFERED, FILE_ANY_ACCESS),
            &in_buffer,
            None,
        )?;

        Ok(())
    }

    fn rm(&self, path: &str) -> io::Result<()> {
        let in_buffer = make_payload(&[path]);

        /* Remove a key from XenStore
         * Input: NUL-terminated CHAR array containing the requested key's path
         * Output: None
         * #define IOCTL_XENIFACE_STORE_REMOVE \
         *     CTL_CODE(FILE_DEVICE_UNKNOWN, 0x803, METHOD_BUFFERED, FILE_ANY_ACCESS)
         */
        self.make_ioctl(
            ctl_code(FILE_DEVICE_UNKNOWN, 0x803, METHOD_BUFFERED, FILE_ANY_ACCESS),
            &in_buffer,
            None,
        )?;

        Ok(())
    }
}

#[cfg(feature = "async-smol")]
#[derive(Clone, Copy, Default)]
pub(crate) struct WatchContext([u8; size_of::<*mut c_void>()]);

#[cfg(feature = "async-smol")]
impl XsWindows {
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self(self.0.try_clone()?))
    }

    pub(crate) fn make_watch(&self, path: &str) -> io::Result<(OwnedHandle, WatchContext)> {
        /* Add a XenStore watch
         * Input: XENIFACE_STORE_ADD_WATCH_IN
         * Output: XENIFACE_STORE_ADD_WATCH_OUT (PVOID)
         * #define IOCTL_XENIFACE_STORE_ADD_WATCH \
         *     CTL_CODE(FILE_DEVICE_UNKNOWN, 0x805, METHOD_BUFFERED, FILE_ANY_ACCESS)
         */
        let c_path = CString::new(path)?;
        let event =
            unsafe { OwnedHandle::from_raw_handle(CreateEventW(None, true, false, None)?.0) };

        /*
         * typedef struct _XENIFACE_STORE_ADD_WATCH_IN {
         *     PCHAR  Path;       /*!< NUL-terminated path to a XenStore key */
         *     ULONG  PathLength; /*!< Size of Path in bytes, including the NUL terminator */
         *     HANDLE Event;      /*!< Handle to an event object that will be signaled when the watch fires */
         * } XENIFACE_STORE_ADD_WATCH_IN, *PXENIFACE_STORE_ADD_WATCH_IN;
         */
        // TODO: Not sure if it would be preferable to use a repr(C) struct.
        let watch_in_bytes = [
            c_path.as_ptr().addr().to_ne_bytes(),
            c_path.as_bytes_with_nul().len().to_ne_bytes(),
            event.as_raw_handle().addr().to_ne_bytes(),
        ];
        let mut context = WatchContext::default();

        self.make_ioctl(
            ctl_code(FILE_DEVICE_UNKNOWN, 0x805, METHOD_BUFFERED, FILE_ANY_ACCESS),
            watch_in_bytes.as_flattened(),
            Some(context.0.as_mut_slice()),
        )?;

        Ok((event, context))
    }

    pub(crate) fn destroy_watch(&self, context: WatchContext) -> io::Result<()> {
        /*
         * Remove a XenStore watch
         * Input: XENIFACE_STORE_REMOVE_WATCH_IN (PVOID)
         * Output: None
         * #define IOCTL_XENIFACE_STORE_REMOVE_WATCH (PVOID)
         *     CTL_CODE(FILE_DEVICE_UNKNOWN, 0x806, METHOD_BUFFERED, FILE_ANY_ACCESS)
         */
        self.make_ioctl(
            ctl_code(FILE_DEVICE_UNKNOWN, 0x806, METHOD_BUFFERED, FILE_ANY_ACCESS),
            &context.0,
            None,
        )?;

        Ok(())
    }
}

unsafe impl Send for XsWindows {}
unsafe impl Sync for XsWindows {}
