//! xenbus device with AsyncWrite/AsyncRead.
//!
use std::{
    fs::File,
    io::{self, Write},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use async_io::Async;
use futures::{AsyncRead, AsyncWrite};

use crate::wire::XENBUS_DEVICE_PATH;

#[derive(Clone)]
pub struct XsDevice(Arc<Async<File>>);

impl XsDevice {
    pub async fn new() -> io::Result<Self> {
        let file = File::options()
            .read(true)
            .write(true)
            .open(XENBUS_DEVICE_PATH)?;

        Ok(Self(Arc::new(Async::new(file)?)))
    }
}

impl AsyncRead for XsDevice {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let pinned = std::pin::pin!(self.0.as_ref());
        pinned.poll_read(cx, buf)
    }
}

impl AsyncWrite for XsDevice {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        // There is a bug in xenbus device that makes poll never yield EPOLLOUT,
        // we need to ignore it and assume that we can always write (xenbus will
        // buffer in that case).
        loop {
            match self.0.get_ref().write(buf) {
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                result => return Poll::Ready(result),
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(self.0.get_ref().flush())
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let pinned = std::pin::pin!(self.0.as_ref());
        pinned.poll_close(cx)
    }
}
