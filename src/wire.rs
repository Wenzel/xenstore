//! Xenstore protocol utilities.

use std::{
    convert::{TryFrom, TryInto},
    io::{self, ErrorKind, Read, Write},
    str::{self, Utf8Error},
};

// TODO: Replace with cfg_match! when available.
//       https://github.com/rust-lang/rust/pull/115416
/// xenbus device path
#[cfg(not(target_os = "windows"))]
pub const XENBUS_DEVICE_PATH: &str = if cfg!(target_os = "freebsd") {
    "/dev/xen/xenstore"
} else if cfg!(target_os = "netbsd") {
    "/kern/xen/xenbus"
} else {
    "/dev/xen/xenbus"
};

pub const XENSTORE_PAYLOAD_MAX: usize = 4096;

#[derive(Clone, Copy, Default, Debug)]
pub struct UnknownMessageType;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XsMessageType {
    Control,
    Directory,
    Read,
    GetPerms,
    Watch,
    Unwatch,
    TransactionStart,
    TransactionEnd,
    Introduce,
    Release,
    GetDomainPath,
    Write,
    Mkdir,
    Rm,
    SetPerms,
    WatchEvent,
    Error,
    IsDomainIntroduced,
    Resume,
    SetTarget,
    ResetWatches,
    DirectoryPart,
}

impl From<XsMessageType> for u32 {
    fn from(val: XsMessageType) -> Self {
        match val {
            XsMessageType::Control => 0,
            XsMessageType::Directory => 1,
            XsMessageType::Read => 2,
            XsMessageType::GetPerms => 3,
            XsMessageType::Watch => 4,
            XsMessageType::Unwatch => 5,
            XsMessageType::TransactionStart => 6,
            XsMessageType::TransactionEnd => 7,
            XsMessageType::Introduce => 8,
            XsMessageType::Release => 9,
            XsMessageType::GetDomainPath => 10,
            XsMessageType::Write => 11,
            XsMessageType::Mkdir => 12,
            XsMessageType::Rm => 13,
            XsMessageType::SetPerms => 14,
            XsMessageType::WatchEvent => 15,
            XsMessageType::Error => 16,
            XsMessageType::IsDomainIntroduced => 17,
            XsMessageType::Resume => 18,
            XsMessageType::SetTarget => 19,
            XsMessageType::ResetWatches => 21,
            XsMessageType::DirectoryPart => 22,
        }
    }
}

impl TryFrom<u32> for XsMessageType {
    type Error = UnknownMessageType;

    fn try_from(value: u32) -> Result<Self, UnknownMessageType> {
        match value {
            0 => Ok(Self::Control),
            1 => Ok(Self::Directory),
            2 => Ok(Self::Read),
            3 => Ok(Self::GetPerms),
            4 => Ok(Self::Watch),
            5 => Ok(Self::Unwatch),
            6 => Ok(Self::TransactionStart),
            7 => Ok(Self::TransactionEnd),
            8 => Ok(Self::Introduce),
            9 => Ok(Self::Release),
            10 => Ok(Self::GetDomainPath),
            11 => Ok(Self::Write),
            12 => Ok(Self::Mkdir),
            13 => Ok(Self::Rm),
            14 => Ok(Self::SetPerms),
            15 => Ok(Self::WatchEvent),
            16 => Ok(Self::Error),
            17 => Ok(Self::IsDomainIntroduced),
            18 => Ok(Self::Resume),
            19 => Ok(Self::SetTarget),
            21 => Ok(Self::ResetWatches),
            22 => Ok(Self::DirectoryPart),
            _ => Err(UnknownMessageType),
        }
    }
}

#[derive(Clone, Debug)]
pub struct XsMessage {
    pub msg_type: XsMessageType,
    pub request_id: u32,
    pub payload: Box<[u8]>,
}

fn read_u32(reader: &mut impl Read) -> Result<u32, io::Error> {
    let mut buffer = [0u8; 4];
    reader.read_exact(&mut buffer)?;
    Ok(u32::from_ne_bytes(buffer))
}

fn write_u32(writer: &mut impl Write, val: u32) -> Result<(), io::Error> {
    writer.write_all(&val.to_ne_bytes())
}

fn parse_nul_string(mut buffer: &[u8]) -> Result<Option<&str>, Utf8Error> {
    // Assuming terminating NUL
    if buffer.is_empty() {
        Ok(None)
    } else {
        // Discard latest NUL character (if present)
        if buffer.last() == Some(&0) {
            buffer = &buffer[..buffer.len() - 1];
        }

        Some(str::from_utf8(buffer)).transpose()
    }
}

impl XsMessage {
    pub fn from_string(msg_type: XsMessageType, request_id: u32, s: &'_ str) -> Self {
        let mut payload: Vec<u8> = Vec::with_capacity(s.len() + 1);
        payload.write_all(s.as_bytes()).unwrap(); // infailble
        payload.push(0);

        Self {
            msg_type,
            request_id,
            payload: payload.into_boxed_slice(),
        }
    }

    pub fn from_string_slice(
        msg_type: XsMessageType,
        request_id: u32,
        strings: &[&'_ str],
    ) -> Self {
        let mut payload: Vec<u8> = Vec::new();

        for s in strings {
            payload.write_all(s.as_bytes()).unwrap(); // infailble
            payload.push(0);
        }

        Self {
            msg_type,
            request_id,
            payload: payload.into_boxed_slice(),
        }
    }

    pub fn write_to(&self, writer: &'_ mut impl Write) -> io::Result<()> {
        if self.payload.len() > XENSTORE_PAYLOAD_MAX {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "Payload is too large (>={4096})",
            ));
        }

        /*
           struct xsd_sockmsg
           {
               uint32_t type;  /* XS_??? */
               uint32_t req_id;/* Request identifier, echoed in daemon's response.  */
               uint32_t tx_id; /* Transaction id (0 if not related to a transaction). */
               uint32_t len;   /* Length of data following this. */

               /* Generally followed by nul-terminated string(s). */
           };
        */
        let mut header = [0u8; 16];
        let mut header_writer = header.as_mut_slice();

        // type
        write_u32(&mut header_writer, self.msg_type as u32)?;

        // req_id
        write_u32(&mut header_writer, self.request_id)?;

        // tx_id (TODO)
        write_u32(&mut header_writer, 0u32)?;

        // len
        write_u32(&mut header_writer, self.payload.len() as u32)?;

        // TODO: Use write_all_vectored when available.
        //       https://github.com/rust-lang/rust/issues/70436
        writer.write_all(&header)?;
        writer.write_all(&self.payload)?;

        Ok(())
    }

    pub fn read_from(reader: &mut impl Read) -> io::Result<Self> {
        let mut raw_msg_header = [0u8; 16]; // 4 * u32
        reader.read_exact(&mut raw_msg_header)?;

        let header_reader = &mut raw_msg_header.as_slice();

        let msg_type = read_u32(header_reader)?;
        let request_id = read_u32(header_reader)?;
        let _tx_id = read_u32(header_reader)?;
        let len = read_u32(header_reader)?;

        let mut payload = vec![0u8; len as _];

        reader.read_exact(&mut payload)?;

        Ok(XsMessage {
            msg_type: msg_type
                .try_into()
                .map_err(|_| io::Error::new(ErrorKind::Unsupported, "Got unknown message type"))?,
            payload: payload.into_boxed_slice(),
            request_id,
        })
    }

    pub fn parse_payload_str(&self) -> Result<Option<&str>, Utf8Error> {
        parse_nul_string(&self.payload)
    }

    pub fn parse_payload_list(&self) -> Result<Vec<&str>, Utf8Error> {
        self.payload
            .split_inclusive(|&c| c == 0)
            .filter_map(|s| parse_nul_string(s).transpose())
            .collect()
    }

    pub fn parse_error(&self) -> io::Error {
        assert_eq!(
            self.msg_type,
            XsMessageType::Error,
            "Tried to parse non-error message"
        );

        let Ok(Some(e_string)) = self.parse_payload_str() else {
            return io::Error::other("Got invalid error code from error payload");
        };

        let kind = match e_string {
            "EINVAL" => io::ErrorKind::InvalidInput,
            "EACCES" => io::ErrorKind::PermissionDenied,
            "EEXIST" => io::ErrorKind::AlreadyExists,
            "EISDIR" => io::ErrorKind::AlreadyExists,
            "ENOENT" => io::ErrorKind::NotFound,
            "ENOMEM" => io::ErrorKind::OutOfMemory,
            "ENOSPC" => io::ErrorKind::OutOfMemory,
            "EIO" => io::ErrorKind::Other,
            "ENOTEMPTY" => io::ErrorKind::InvalidInput,
            "ENOSYS" => io::ErrorKind::Unsupported,
            "EROFS" => io::ErrorKind::PermissionDenied,
            "EBUSY" => io::ErrorKind::AlreadyExists,
            "EAGAIN" => io::ErrorKind::WouldBlock,
            "EISCONN" => io::ErrorKind::AddrInUse,
            "E2BIG" => io::ErrorKind::InvalidData,
            "EPERM" => io::ErrorKind::PermissionDenied,
            _ => io::ErrorKind::Other,
        };

        io::Error::new(kind, format!("XS interface error {e_string}"))
    }
}
