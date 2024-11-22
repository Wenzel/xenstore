//! Tokio async implementation.
//!
//! Alike Unix implementation, uses either xenstored socket or xenbus/xenstore device.
//!
//! This implementation uses a underlying task to multiplex the concurrent
//! accesses and manage watchers. If this underlying task dies (e.g dead xenstore socket),
//! all future operations will fail with [io::ErrorKind::BrokenPipe] and all watchers
//! will yield [None].

mod device;
mod interface;
mod wire_async;

use std::{
    env,
    io::{self, ErrorKind},
    pin::Pin,
    task::{Context, Poll},
};

use futures::Stream;
use tokio::{
    net::UnixStream,
    sync::{mpsc, oneshot},
};

use interface::{launch_xenstore_task, XsTokioMessage, XsTokioRequest, XsWatchToken};

use crate::{
    wire::{XsMessage, XsMessageType},
    AsyncWatch, AsyncXs,
};

/// Tokio Xenstore implementation.
///
/// It can be cloned and used concurrently by multiple tasks.
#[derive(Clone, Debug)]
pub struct XsTokio(mpsc::UnboundedSender<XsTokioMessage>);

impl XsTokio {
    /// Try to open Xenstore interface.
    /// Attempt in order :
    ///  - `/run/xenstored/socket` (unix domain socket)
    ///  - [crate::wire::XENBUS_DEVICE_PATH] (xenstore device)
    pub async fn new() -> io::Result<Self> {
        let xsd_path =
            env::var("XENSTORED_PATH").unwrap_or_else(|_| "/run/xenstored/socket".to_string());

        // Use xenstored socket first
        if let Ok(stream) = UnixStream::connect(xsd_path).await {
            return Ok(Self(launch_xenstore_task(stream)));
        }

        Ok(Self(launch_xenstore_task(device::XsDevice::new().await?)))
    }

    async fn transmit_request(&self, request: XsMessage) -> io::Result<XsMessage> {
        let (response_sender, response_receiver) = oneshot::channel();
        let req_msg_type = request.msg_type;

        self.0
            .send(XsTokioMessage::Request(XsTokioRequest {
                request,
                response_sender,
            }))
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

impl AsyncXs for XsTokio {
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

/// Tokio watch object.
pub struct XsTokioWatch {
    event_receiver: mpsc::Receiver<Box<str>>,
    tokio_channel: mpsc::UnboundedSender<XsTokioMessage>,
    token: XsWatchToken,
}

impl Stream for XsTokioWatch {
    type Item = Box<str>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.event_receiver.poll_recv(cx)
    }
}

impl Drop for XsTokioWatch {
    fn drop(&mut self) {
        // Try to unsubscribe upstream (to not leak the watch token/state).
        // If it fails, it means that the upper backend has died.
        self.tokio_channel
            .send(XsTokioMessage::WatchUnsubscribe(self.token))
            .ok();
    }
}

impl AsyncWatch for XsTokio {
    async fn watch(&self, path: &str) -> io::Result<impl Stream<Item = Box<str>> + 'static> {
        let (event_sender, event_receiver) = mpsc::channel(8);
        let (result_channel, result_receiver) = oneshot::channel();

        self.0
            .send(XsTokioMessage::WatchSubscribe {
                path: path.to_string().into_boxed_str(),
                event_sender,
                result_channel,
            })
            .map_err(|e| io::Error::new(ErrorKind::BrokenPipe, e))?;

        let token = result_receiver
            .await
            .map_err(|e| io::Error::new(ErrorKind::BrokenPipe, e))??;

        Ok(XsTokioWatch {
            event_receiver,
            token,
            tokio_channel: self.0.clone(),
        })
    }
}
