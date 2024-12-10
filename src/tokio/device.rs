//! xenbus device with AsyncWrite/AsyncRead.
//!
//! As tokio doesn't provide [AsyncWrite]/[AsyncRead] [1] on AsyncFd,
//! we implement it using a with non-blocking std::fd::File to have
//! asynchronous and concurrent read/write on the xenbus device.
//!
//! We need to rely on it as [tokio::fs::File] doesn't support concurrent
//! read/write (after [tokio::io::split]).
//!
//! See https://github.com/tokio-rs/tokio/issues/5785

use std::{
    fs::File,
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    pin::Pin,
    task::{ready, Context, Poll},
};

use libc::O_NONBLOCK;
use tokio::{
    io::{self, unix::AsyncFd, AsyncRead, AsyncWrite, Error},
    task,
};

use crate::wire::XENBUS_DEVICE_PATH;

pub struct XsDevice(AsyncFd<File>);

impl XsDevice {
    pub async fn new() -> io::Result<Self> {
        let file = task::spawn_blocking(|| {
            File::options()
                .read(true)
                .write(true)
                .custom_flags(O_NONBLOCK)
                .open(XENBUS_DEVICE_PATH)
        })
        .await??;

        Ok(Self(AsyncFd::new(file)?))
    }
}

impl AsyncRead for XsDevice {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // From https://docs.rs/tokio/1.41.1/tokio/io/unix/struct.AsyncFd.html
        loop {
            let mut guard = ready!(self.0.poll_read_ready(cx))?;

            let unfilled = buf.initialize_unfilled();
            match guard.try_io(|inner| inner.get_ref().read(unfilled)) {
                Ok(Ok(len)) => {
                    buf.advance(len);
                    return Poll::Ready(Ok(()));
                }
                Ok(Err(err)) => return Poll::Ready(Err(err)),
                Err(_would_block) => continue,
            }
        }
    }
}

impl AsyncWrite for XsDevice {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
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

    fn poll_flush(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Poll::Ready(self.0.get_mut().flush())
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        self.poll_flush(cx)
    }
}
