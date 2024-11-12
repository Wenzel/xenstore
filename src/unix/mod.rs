//! Unix blocking implementation.
//!
//! Uses either xenstored socket or xenbus/xenstore device.

mod interface;

use std::io;

use crate::{
    wire::{XsMessage, XsMessageType},
    Xs,
};

/// Unix Xenstore implementation.
pub struct XsUnix(interface::XsUnixInterface);

impl XsUnix {
    /// Try to open Xenstore interface.
    /// Attempt in order :
    ///  - `/run/xenstored/socket` (unix domain socket)
    ///  - [crate::wire::XENBUS_DEVICE_PATH] (xenstore device)
    pub fn new() -> io::Result<Self> {
        Ok(Self(interface::XsUnixInterface::new()?))
    }

    fn transmit_request(&mut self, request: XsMessage) -> io::Result<XsMessage> {
        request.write_to(&mut self.0)?;

        let response = XsMessage::read_from(&mut self.0)?;

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
    fn directory(&mut self, path: &str) -> io::Result<Vec<Box<str>>> {
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

    fn read(&mut self, path: &str) -> io::Result<Box<str>> {
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

    fn write(&mut self, path: &str, data: &str) -> io::Result<()> {
        self.transmit_request(XsMessage::from_string_slice(
            XsMessageType::Write,
            0,
            &[path, data],
        ))?;

        Ok(())
    }

    fn rm(&mut self, path: &str) -> io::Result<()> {
        self.transmit_request(XsMessage::from_string(XsMessageType::Rm, 0, path))?;

        Ok(())
    }
}
