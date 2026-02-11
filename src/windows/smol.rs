use std::{
    future::{self, Future},
    io::{self, ErrorKind},
    marker::PhantomData,
    os::windows::io::{AsRawHandle, OwnedHandle},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use async_executor::Executor;
use async_io::os::windows::Waitable;
use futures::{ready, Stream};
use windows::Win32::{Foundation::HANDLE, System::Threading::ResetEvent};

use crate::{AsyncWatch, AsyncXs, AsyncXsPerm, Xs, XsPermission};

use super::{WatchContext, XsWindows};

// Please keep it in parity with [crate::smol::XsSmol]
#[derive(Clone, Debug)]
pub struct XsSmol<'a>(Arc<XsWindows>, PhantomData<&'a ()>);

impl XsSmol<'_> {
    pub async fn new(_: &Executor<'_>) -> io::Result<Self> {
        Ok(Self(Arc::new(XsWindows::new()?), PhantomData))
    }
}

// TODO: Find a way to use overlapped IO instead.
impl AsyncXs for XsSmol<'_> {
    fn directory(&self, path: &str) -> impl Future<Output = io::Result<Vec<Box<str>>>> + Send {
        future::ready(self.0.directory(path))
    }

    fn read(&self, path: &str) -> impl Future<Output = io::Result<Box<str>>> + Send {
        future::ready(self.0.read(path))
    }

    fn write(&self, path: &str, data: &str) -> impl Future<Output = io::Result<()>> + Send {
        future::ready(self.0.write(path, data))
    }

    fn rm(&self, path: &str) -> impl Future<Output = io::Result<()>> + Send {
        future::ready(self.0.rm(path))
    }
}

pub struct XsWindowsWatch {
    device: XsWindows,
    waitable: Waitable<OwnedHandle>,
    context: WatchContext,
    path: Box<str>,
}

impl Stream for XsWindowsWatch {
    type Item = Box<str>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(ready!(self.waitable.poll_ready(cx)).ok().map(|_| {
            unsafe {
                ResetEvent(HANDLE(self.waitable.get_ref().as_raw_handle()))
                    .inspect_err(|e| log::error!("Unable to reset event handle: {e}"))
                    .ok()
            };
            self.path.clone()
        }))
    }
}

impl Drop for XsWindowsWatch {
    fn drop(&mut self) {
        if let Err(e) = self.device.destroy_watch(self.context) {
            log::warn!("Unable to destroy watch object {e}")
        }
    }
}

impl AsyncWatch for XsSmol<'_> {
    async fn watch(
        &self,
        path: &str,
    ) -> io::Result<impl Stream<Item = Box<str>> + Unpin + 'static> {
        // We want a clone of the device handle to be able to destroy the watch.
        let device = self.0.try_clone()?;
        let (event_handle, context) = self.0.make_watch(path)?;
        let waitable = Waitable::new(event_handle)?;

        Ok(XsWindowsWatch {
            device,
            context,
            waitable,
            path: path.into(),
        })
    }
}

impl AsyncXsPerm for XsSmol<'_> {
    async fn get_perms(&self, _: &str) -> io::Result<Vec<XsPermission>> {
        Err(io::Error::new(ErrorKind::Unsupported, "Unimplemented"))
    }

    async fn set_perms(&self, _: &str, _: &[XsPermission]) -> io::Result<()> {
        Err(io::Error::new(ErrorKind::Unsupported, "Unimplemented"))
    }
}
