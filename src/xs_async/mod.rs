//! Generic async implementation.
//!
//! This implementation uses a underlying task to multiplex the concurrent
//! accesses and manage watchers. If this underlying task dies (e.g dead xenstore socket),
//! all future operations will fail with [io::ErrorKind::BrokenPipe] and all watchers
//! will yield [None].

mod interface;
mod wire_async;

use core::{
    pin::Pin,
    task::{Context, Poll},
};
use futures::{
    channel::oneshot,
    io::{self, ErrorKind},
    FutureExt, Stream,
};
use std::str::FromStr;

use interface::{XsAsyncMessage, XsAsyncRequest, XsWatchToken};

use crate::{
    wire::{XsMessage, XsMessageType},
    AsyncWatch, AsyncXs, AsyncXsPerm, XsPermission,
};

pub use interface::XsAsyncState;

/// Generic async Xenstore implementation.
///
/// It can be cloned and used concurrently by multiple tasks.
#[derive(Clone, Debug)]
pub struct XsAsyncImpl(flume::Sender<XsAsyncMessage>);

impl XsAsyncImpl {
    pub fn new(tx: flume::Sender<XsAsyncMessage>) -> io::Result<Self> {
        Ok(Self(tx))
    }

    async fn transmit_request(&self, request: XsMessage) -> io::Result<XsMessage> {
        let (response_sender, response_receiver) = oneshot::channel();
        let req_msg_type = request.msg_type;

        self.0
            .send_async(XsAsyncMessage::Request(XsAsyncRequest {
                request,
                response_sender,
            }))
            .await
            .map_err(|e| io::Error::new(ErrorKind::BrokenPipe, e))?;

        let response = response_receiver
            .await
            .map_err(|e| io::Error::new(ErrorKind::BrokenPipe, e))?;

        match response.msg_type {
            // Response type must match request.
            msg_type if msg_type == req_msg_type => Ok(response),
            XsMessageType::Error => Err(response.parse_error()),
            msg_type => Err(io::Error::new(
                ErrorKind::InvalidData,
                format!("Got unrelated response ({msg_type:?})"),
            )),
        }
    }
}

impl AsyncXs for XsAsyncImpl {
    async fn directory(&self, path: &str) -> io::Result<Vec<Box<str>>> {
        let response = self
            .transmit_request(XsMessage::from_string(XsMessageType::Directory, 0, path))
            .await?;

        Ok(response
            .parse_payload_list()
            .map_err(|e| io::Error::new(ErrorKind::InvalidData, e))?
            // convert &str to Box<str>
            .iter()
            .map(|s| s.to_string().into_boxed_str())
            .collect())
    }

    async fn read(&self, path: &str) -> io::Result<Box<str>> {
        let response = self
            .transmit_request(XsMessage::from_string(XsMessageType::Read, 0, path))
            .await?;

        Ok(response
            .parse_payload_str()
            .map_err(|e| io::Error::new(ErrorKind::InvalidData, e))?
            .unwrap_or_default()
            // convert &str to Box<str>
            .to_string()
            .into_boxed_str())
    }

    async fn write(&self, path: &str, data: &str) -> io::Result<()> {
        self.transmit_request(XsMessage::from_string_slice(
            XsMessageType::Write,
            0,
            &[path, data],
            false,
        ))
        .await?;

        Ok(())
    }

    async fn rm(&self, path: &str) -> io::Result<()> {
        self.transmit_request(XsMessage::from_string(XsMessageType::Rm, 0, path))
            .await?;

        Ok(())
    }
}

impl AsyncXsPerm for XsAsyncImpl {
    async fn get_perms(&self, path: &str) -> io::Result<Vec<XsPermission>> {
        let response = self
            .transmit_request(XsMessage::from_string(XsMessageType::GetPerms, 0, path))
            .await?;

        let payloads = response
            .parse_payload_list()
            .map_err(|e| io::Error::new(ErrorKind::InvalidData, e))?;

        payloads
            .iter()
            .map(|s| XsPermission::from_str(s).map_err(io::Error::other))
            .collect()
    }

    async fn set_perms(&self, path: &str, perms: &[XsPermission]) -> io::Result<()> {
        // Build a parameter list for <path>|<perm-as-string>|+?
        let perms_strings: Vec<String> = perms.iter().map(ToString::to_string).collect();

        let mut perms_str: Vec<&str> = Vec::new();
        perms_str.reserve_exact(1 + perms.len());

        perms_str.push(path);
        perms_strings.iter().for_each(|s| perms_str.push(s));

        self.transmit_request(XsMessage::from_string_slice(
            XsMessageType::SetPerms,
            0,
            &perms_str,
            true,
        ))
        .await?;

        Ok(())
    }
}

/// Tokio watch object.
pub struct XsAsyncWatch {
    event_receiver: flume::Receiver<Box<str>>,
    async_channel: flume::Sender<XsAsyncMessage>,
    token: XsWatchToken,
}

impl Stream for XsAsyncWatch {
    type Item = Box<str>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let result = futures::ready!(self.event_receiver.recv_async().poll_unpin(cx));

        Poll::Ready(result.ok())
    }
}

impl Drop for XsAsyncWatch {
    // Try to unsubscribe upstream (to not leak the watch token/state).
    // If it fails, it means that the upper backend has died.
    fn drop(&mut self) {
        self.async_channel
            .send(XsAsyncMessage::WatchUnsubscribe(self.token))
            .ok();
    }
}

impl AsyncWatch for XsAsyncImpl {
    async fn watch(&self, path: &str) -> io::Result<impl Stream<Item = Box<str>> + 'static> {
        let (event_sender, event_receiver) = flume::bounded(8);
        let (result_channel, result_receiver) = oneshot::channel();

        self.0
            .send_async(XsAsyncMessage::WatchSubscribe {
                path: path.to_string().into_boxed_str(),
                event_sender,
                result_channel,
            })
            .await
            .map_err(|e| io::Error::new(ErrorKind::BrokenPipe, e))?;

        let token = result_receiver
            .await
            .map_err(|e| io::Error::new(ErrorKind::BrokenPipe, e))??;

        Ok(XsAsyncWatch {
            event_receiver,
            token,
            async_channel: self.0.clone(),
        })
    }
}
