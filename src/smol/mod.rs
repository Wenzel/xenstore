//! smol/async-std async implementation.
//!
//! Alike Unix implementation, uses either xenstored socket or xenbus/xenstore device.
//!
//! This implementation uses a underlying task to multiplex the concurrent
//! accesses and manage watchers. If this underlying task dies (e.g dead xenstore socket),
//! all future operations will fail with [io::ErrorKind::BrokenPipe] and all watchers
//! will yield [None].

// TODO: Use at some point Spawn trait.
// See https://github.com/smol-rs/async-executor/issues/25

mod device;

use std::{env, io, marker::PhantomData};

use futures::{AsyncRead, AsyncWrite, Stream};
use log::{debug, error};

use crate::{
    wire::XsMessage,
    xs_async::{XsAsyncImpl, XsAsyncState},
    AsyncWatch, AsyncXs, AsyncXsPerm, XsPermission,
};

use async_executor::Executor;
use async_net::unix::UnixStream;

/// Smol Xenstore implementation.
///
/// It can be cloned and used concurrently by multiple tasks.
#[derive(Clone, Debug)]
pub struct XsSmol<'a>(XsAsyncImpl, PhantomData<Executor<'a>>);

impl<'a> XsSmol<'a> {
    /// Try to open Xenstore interface.
    /// Attempt in order :
    ///  - `/run/xenstored/socket` (unix domain socket)
    ///  - [crate::wire::XENBUS_DEVICE_PATH] (xenstore device)
    pub async fn new(executor: &Executor<'a>) -> io::Result<Self> {
        let xsd_path =
            env::var("XENSTORED_PATH").unwrap_or_else(|_| "/run/xenstored/socket".to_string());

        // Use xenstored socket first
        if let Ok(stream) = UnixStream::connect(xsd_path).await {
            return Ok(Self(
                run_xenstore_task(executor, stream.clone(), stream)?,
                PhantomData,
            ));
        }

        let device = device::XsDevice::new().await?;
        Ok(Self(
            run_xenstore_task(executor, device.clone(), device)?,
            PhantomData,
        ))
    }
}

impl AsyncXs for XsSmol<'_> {
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

impl AsyncWatch for XsSmol<'_> {
    async fn watch(
        &self,
        path: &str,
    ) -> io::Result<impl Stream<Item = Box<str>> + Unpin + 'static> {
        self.0.watch(path).await
    }
}

impl AsyncXsPerm for XsSmol<'_> {
    async fn get_perms(&self, path: &str) -> io::Result<Vec<XsPermission>> {
        self.0.get_perms(path).await
    }

    async fn set_perms(&self, path: &str, perms: &[XsPermission]) -> io::Result<()> {
        self.0.set_perms(path, perms).await
    }
}

pub fn run_xenstore_task<R, W>(
    executor: &Executor<'_>,
    mut xs_read: R,
    mut xs_write: W,
) -> io::Result<XsAsyncImpl>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    // Xenstore response channel
    let (response_tx, xs_receiver) = flume::bounded(4);

    // Xenstore request channel
    let (xs_sender, request_rx) = flume::bounded(4);

    // Xenstore Rust channel
    let (xs_async_tx, xs_async_rx) = flume::unbounded();

    // Message receiver task
    executor
        .spawn(async move {
            while let Ok(message) = XsMessage::read_message_async(&mut xs_read).await {
                debug!("< {message:?}");

                if response_tx.send_async(message).await.is_err() {
                    break;
                }
            }

            error!("Read message failure");
        })
        .detach();

    // Message sender task
    executor
        .spawn(async move {
            while let Ok(message) = request_rx.recv_async().await {
                debug!("> {message:?}");

                if let Err(e) = XsMessage::write_message_async(&message, &mut xs_write).await {
                    error!("Write message failure {e}");
                    break;
                }
            }
        })
        .detach();

    let state = XsAsyncState::default();
    executor
        .spawn(state.run(xs_async_rx, xs_receiver, xs_sender))
        .detach();

    XsAsyncImpl::new(xs_async_tx)
}
