//! Xeniface device discovery utilities.
//!
use log::{error, warn};
use windows::{
    core::{Result, GUID},
    Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
        SetupDiGetDeviceInterfaceDetailW, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, HDEVINFO,
        SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W,
    },
};

pub const GUID_INTERFACE_XENIFACE: GUID = GUID::from_values(
    0xb2cfb085,
    0xaa5e,
    0x47e1,
    [0x8b, 0xf7, 0x97, 0x93, 0xf3, 0x15, 0x45, 0x65],
);

const MAX_INTERFACE_DETAIL_PATH_LEN: usize = 4094;

// Extended SP_DEVICE_INTERFACE_DATA_DETAIL_W (fixed flex array at 4094)
// Maximum path is 4094 characters.
// Hopefully, we would have "never" a that large path.
#[repr(C)]
struct ExtendedDataDetail {
    cb_size: u32,
    path: [u16; MAX_INTERFACE_DETAIL_PATH_LEN],
}

impl Default for ExtendedDataDetail {
    fn default() -> Self {
        Self {
            cb_size: Default::default(),
            path: [0; MAX_INTERFACE_DETAIL_PATH_LEN],
        }
    }
}

impl ExtendedDataDetail {
    // ExtendedDataDetail is ABI-compatible with SP_DEVICE_INTERFACE_DETAIL_DATA_W.
    // We just need to ensure that required length < size_of::<ExtendedDataDetail>().
    fn as_data_detail_ptr(&mut self) -> *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W {
        &mut *self as *mut _ as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W
    }
}

/// Set of device sharing the GUID.
pub struct DeviceInfoList {
    info: HDEVINFO,
    class_guid: GUID,
}

impl DeviceInfoList {
    pub fn new(class_guid: GUID) -> Result<Self> {
        Ok(Self {
            info: unsafe {
                SetupDiGetClassDevsW(
                    Some(&class_guid),
                    None,
                    None,
                    DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
                )
            }?,
            class_guid,
        })
    }

    pub fn iter(&self) -> DeviceInfoIterator<'_> {
        DeviceInfoIterator {
            list: self,
            index: 0,
            buffer: Box::default(),
        }
    }
}

impl Drop for DeviceInfoList {
    fn drop(&mut self) {
        if let Err(e) = unsafe { SetupDiDestroyDeviceInfoList(self.info) } {
            warn!("Unable to destroy device info list {e}");
        }
    }
}

pub struct DeviceInfoIterator<'a> {
    list: &'a DeviceInfoList,
    index: u32,
    buffer: Box<ExtendedDataDetail>,
}

/// Iterator of device info paths in WTF16 encoding.
impl Iterator for DeviceInfoIterator<'_> {
    type Item = Box<[u16]>;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            let mut data = SP_DEVICE_INTERFACE_DATA {
                cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
                ..Default::default()
            };

            // Iterate and take the next one that "works".
            while SetupDiEnumDeviceInterfaces(
                self.list.info,
                None,
                &self.list.class_guid,
                self.index,
                &mut data,
            )
            .is_ok()
            {
                self.index += 1;
                let mut length = 0;

                // Get the length of the interface detail.
                SetupDiGetDeviceInterfaceDetailW(
                    self.list.info,
                    &mut data,
                    None,
                    0,
                    Some(&mut length),
                    None,
                ) // it will fail but we only want to know length
                .ok();

                if (length as usize) > size_of::<ExtendedDataDetail>() {
                    warn!(
                        "interface detail too large ! ({} > {})",
                        length,
                        size_of::<ExtendedDataDetail>()
                    );
                    continue;
                }

                self.buffer.cb_size = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;

                if let Err(e) = SetupDiGetDeviceInterfaceDetailW(
                    self.list.info,
                    &mut data,
                    Some(self.buffer.as_data_detail_ptr()),
                    length,
                    None,
                    None,
                ) {
                    error!(
                        "SetupDiGetDeviceInterfaceDetailW(index = {}) failure: {e:?}",
                        self.index - 1
                    );
                    continue;
                };

                return Some(self.buffer.path.into());
            }

            None
        }
    }
}
