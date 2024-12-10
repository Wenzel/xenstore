//! Some wire utilities for async.
use std::{
    convert::TryInto,
    io::{ErrorKind, Read, Write},
};

use tokio::io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::wire::{XsMessage, XENSTORE_PAYLOAD_MAX};

fn read_u32(reader: &mut impl Read) -> io::Result<u32> {
    let mut buffer = [0u8; 4];
    reader.read_exact(&mut buffer)?;
    Ok(u32::from_ne_bytes(buffer))
}

fn write_u32(writer: &mut impl Write, val: u32) -> io::Result<()> {
    writer.write_all(&val.to_ne_bytes())
}

impl XsMessage {
    pub async fn read_message_async<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<Self> {
        let mut raw_msg_header = [0u8; 16]; // 4 * u32
        reader.read_exact(&mut raw_msg_header).await?;

        let header_reader = &mut raw_msg_header.as_slice();

        let msg_type = read_u32(header_reader)?;
        let request_id = read_u32(header_reader)?;
        let _tx_id = read_u32(header_reader)?;
        let len = read_u32(header_reader)?;

        let mut payload = vec![0u8; len as _];

        reader.read_exact(&mut payload).await?;

        Ok(XsMessage {
            msg_type: msg_type
                .try_into()
                .map_err(|_| io::Error::new(ErrorKind::Unsupported, "Got unknown message type"))?,
            payload: payload.into_boxed_slice(),
            request_id,
        })
    }

    pub async fn write_message_async<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
    ) -> io::Result<()> {
        if self.payload.len() > XENSTORE_PAYLOAD_MAX {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "Payload is too large (>4096)",
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

        // tx_id (unused)
        write_u32(&mut header_writer, 0u32)?;

        // len
        write_u32(&mut header_writer, self.payload.len() as u32)?;

        writer.write_all(&header).await?;
        writer.write_all(&self.payload).await?;

        Ok(())
    }
}
