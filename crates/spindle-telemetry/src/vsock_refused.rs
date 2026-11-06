//! vsock off Linux: the same types as on Linux, every one of whose constructors answers
//! [`io::ErrorKind::Unsupported`]. `vsock:<cid>:<port>` still parses and prints, so a document
//! naming one reads the same everywhere; only connecting and listening are refused.
//!
//! vsock reaches a microVM guest, and the microVM launchers run on Linux only.

use std::convert::Infallible;
use std::io::{self, Read, Write};
use std::time::Duration;

/// Any context id, for a listener: `VMADDR_CID_ANY`.
pub const CID_ANY: u32 = u32::MAX;
/// Any port, for a listener that lets the kernel choose: `VMADDR_PORT_ANY`.
pub const PORT_ANY: u32 = u32::MAX;

/// What every vsock operation answers off Linux.
pub const REFUSAL: &str = "vsock is available on Linux only";

fn refused() -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, REFUSAL)
}

/// A connected vsock stream; none exists off Linux.
#[derive(Debug)]
pub struct VsockStream(Infallible);

impl VsockStream {
    /// Refused: [`REFUSAL`].
    pub fn connect_within(_cid: u32, _port: u32, _within: Duration) -> io::Result<Self> {
        Err(refused())
    }

    /// Writes give up after `timeout`; `None` waits for as long as it takes.
    pub fn set_write_timeout(&self, _timeout: Option<Duration>) -> io::Result<()> {
        match self.0 {}
    }

    /// Reads give up after `timeout`; `None` waits for as long as it takes.
    pub fn set_read_timeout(&self, _timeout: Option<Duration>) -> io::Result<()> {
        match self.0 {}
    }
}

impl Read for VsockStream {
    fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
        match self.0 {}
    }
}

impl Write for VsockStream {
    fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
        match self.0 {}
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.0 {}
    }
}

/// A listening vsock socket on the host; none exists off Linux.
#[derive(Debug)]
pub struct VsockListener(Infallible);

impl VsockListener {
    /// Refused: [`REFUSAL`].
    pub fn bind(_port: u32) -> io::Result<Self> {
        Err(refused())
    }

    /// Refused: [`REFUSAL`].
    pub fn bind_any_port() -> io::Result<Self> {
        Self::bind(PORT_ANY)
    }

    /// The port it listens on.
    pub fn port(&self) -> u32 {
        match self.0 {}
    }

    /// Accepts one connection, waiting at most `within`.
    pub fn accept_within(&self, _within: Duration) -> io::Result<VsockStream> {
        match self.0 {}
    }
}
