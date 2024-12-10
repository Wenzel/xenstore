use std::{
    env,
    fs::File,
    io::{self, Read, Write},
    os::unix::net::UnixStream,
};

use crate::wire::XENBUS_DEVICE_PATH;

/// Raw xenstore interface (speaks [crate::wire] protocol).
#[derive(Debug)]
pub enum XsUnixInterface {
    Socket(UnixStream),
    Device(File),
}

impl XsUnixInterface {
    pub fn new() -> io::Result<Self> {
        let xsd_path =
            env::var("XENSTORED_PATH").unwrap_or_else(|_| "/run/xenstored/socket".to_string());

        // Use xenstored first
        if let Ok(stream) = UnixStream::connect(xsd_path) {
            return Ok(XsUnixInterface::Socket(stream));
        }

        Ok(XsUnixInterface::Device(
            File::options()
                .read(true)
                .write(true)
                .open(XENBUS_DEVICE_PATH)?,
        ))
    }
}

impl Write for XsUnixInterface {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            XsUnixInterface::Socket(unix_stream) => unix_stream.write(buf),
            XsUnixInterface::Device(file) => file.write(buf),
        }
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        match self {
            XsUnixInterface::Socket(unix_stream) => unix_stream.write_all(buf),
            XsUnixInterface::Device(file) => file.write_all(buf),
        }
    }

    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        match self {
            XsUnixInterface::Socket(unix_stream) => unix_stream.write_vectored(bufs),
            XsUnixInterface::Device(file) => file.write_vectored(bufs),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            XsUnixInterface::Socket(unix_stream) => unix_stream.flush(),
            XsUnixInterface::Device(file) => file.flush(),
        }
    }
}

impl Read for XsUnixInterface {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            XsUnixInterface::Socket(unix_stream) => unix_stream.read(buf),
            XsUnixInterface::Device(file) => file.read(buf),
        }
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        match self {
            XsUnixInterface::Socket(unix_stream) => unix_stream.read_exact(buf),
            XsUnixInterface::Device(file) => file.read_exact(buf),
        }
    }
}
