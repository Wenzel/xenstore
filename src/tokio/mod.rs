//! Tokio async implementation.
//!
//! Alike Unix implementation, uses either xenstored socket or xenbus/xenstore device.
//!
//! This implementation uses a underlying task to multiplex the concurrent
//! accesses and manage watchers. If this underlying task dies (e.g dead xenstore socket),
//! all future operations will fail with [io::ErrorKind::BrokenPipe] and all watchers
//! will yield [None].

mod device;

use std::{env, io};

use futures::Stream;
use log::{debug, error};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::UnixStream,
};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::{
    wire::XsMessage,
    xs_async::{XsAsyncImpl, XsAsyncState},
    AsyncWatch, AsyncXs, AsyncXsPerm, XsPermission,
};

/// Tokio Xenstore implementation.
///
/// It can be cloned and used concurrently by multiple tasks.
#[derive(Clone, Debug)]
pub struct XsTokio(XsAsyncImpl);

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
            return Ok(Self(launch_xenstore_task(stream)?));
        }

        Ok(Self(launch_xenstore_task(device::XsDevice::new().await?)?))
    }
}

impl AsyncXs for XsTokio {
    async fn directory(&self, path: &str) -> io::Result<Vec<Box<str>>> {
        self.0.directory(path).await
    }

    async fn read(&self, path: &str) -> io::Result<Box<str>> {
        self.0.read(path).await
    }

    async fn write(&self, path: &str, data: &str) -> io::Result<()> {
        self.0.write(path, data).await
    }

    async fn rm(&self, path: &str) -> io::Result<()> {
        self.0.rm(path).await
    }
}

impl AsyncWatch for XsTokio {
    async fn watch(
        &self,
        path: &str,
    ) -> io::Result<impl Stream<Item = Box<str>> + Unpin + 'static> {
        self.0.watch(path).await
    }
}

impl AsyncXsPerm for XsTokio {
    async fn get_perms(&self, path: &str) -> io::Result<Vec<XsPermission>> {
        self.0.get_perms(path).await
    }

    async fn set_perms(&self, path: &str, perms: &[XsPermission]) -> io::Result<()> {
        self.0.set_perms(path, perms).await
    }
}

pub fn launch_xenstore_task<S>(xs_stream: S) -> io::Result<XsAsyncImpl>
where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let (rx, tx) = tokio::io::split(xs_stream);

    // Xenstore response channel
    let (response_tx, xs_receiver) = flume::bounded(4);

    // Xenstore request channel
    let (xs_sender, request_rx) = flume::bounded(4);

    // Xenstore Rust channel
    let (xs_async_tx, xs_async_rx) = flume::unbounded();

    // Message receiver task
    tokio::spawn(async move {
        let mut rx = rx.compat();
        while let Ok(message) = XsMessage::read_message_async(&mut rx).await {
            debug!("< {message:?}");

            if response_tx.send_async(message).await.is_err() {
                break;
            }
        }

        error!("Read message failure");
    });

    // Message sender task
    tokio::spawn(async move {
        let mut tx = tx.compat_write();
        while let Ok(message) = request_rx.recv_async().await {
            debug!("> {message:?}");

            if let Err(e) = XsMessage::write_message_async(&message, &mut tx).await {
                error!("Write message failure {e}");
                break;
            }
        }
    });

    let state = XsAsyncState::default();
    tokio::spawn(state.run(xs_async_rx, xs_receiver, xs_sender));

    XsAsyncImpl::new(xs_async_tx)
}
