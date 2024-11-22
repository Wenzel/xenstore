//! Unix blocking implementation.
//!
//! Uses either xenstored socket or xenbus/xenstore device.

mod interface;

use std::{cell::RefCell, io, ops::DerefMut};

use crate::{
    wire::{XsMessage, XsMessageType},
    Xs,
};

/// Unix Xenstore implementation.
pub struct XsUnix(RefCell<interface::XsUnixInterface>);

impl XsUnix {
    /// Try to open Xenstore interface.
    /// Attempt in order :
    ///  - `/run/xenstored/socket` (unix domain socket)
    ///  - [crate::wire::XENBUS_DEVICE_PATH] (xenstore device)
    pub fn new() -> io::Result<Self> {
        Ok(Self(RefCell::new(interface::XsUnixInterface::new()?)))
    }

    fn transmit_request(&self, request: XsMessage) -> io::Result<XsMessage> {
        let mut writer = self.0.borrow_mut();
        request.write_to(writer.deref_mut())?;

        let response = XsMessage::read_from(writer.deref_mut())?;

        match response.msg_type {
            // Response type must match request.
            msg_type if msg_type == request.msg_type => Ok(response),
            XsMessageType::Error => Err(response.parse_error()),
            msg_type => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Got unrelated response ({msg_type:?})"),
            )),
        }
    }
}

impl Xs for XsUnix {
    fn directory(&self, path: &str) -> io::Result<Vec<Box<str>>> {
        // TODO: If we receive E2BIG, it means that the directory listing is too long,
        //       and that we should use DIRECTORY_PART.
        let response =
            self.transmit_request(XsMessage::from_string(XsMessageType::Directory, 0, path))?;

        Ok(response
            .parse_payload_list()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
            // convert &str to Box<str>
            .iter()
            .map(|s| s.to_string().into_boxed_str())
            .collect())
    }

    fn read(&self, path: &str) -> io::Result<Box<str>> {
        let response =
            self.transmit_request(XsMessage::from_string(XsMessageType::Read, 0, path))?;

        Ok(response
            .parse_payload_str()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
            .unwrap_or_default()
            // convert &str to Box<str>
            .to_string()
            .into_boxed_str())
    }

    fn write(&self, path: &str, data: &str) -> io::Result<()> {
        self.transmit_request(XsMessage::from_string_slice(
            XsMessageType::Write,
            0,
            &[path, data],
        ))?;

        Ok(())
    }

    fn rm(&self, path: &str) -> io::Result<()> {
        self.transmit_request(XsMessage::from_string(XsMessageType::Rm, 0, path))?;

        Ok(())
    }
}
