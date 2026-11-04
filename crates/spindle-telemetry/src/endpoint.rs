//! Where an instance sends its frames: the value of `SPINDLE_TELEMETRY`.

use std::fmt;
use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use crate::vsock::VsockStream;

/// The variable a launcher gives an instance its endpoint in.
pub const ENV_TELEMETRY: &str = "SPINDLE_TELEMETRY";
/// The variable that sets the sample period, in milliseconds; [`DEFAULT_PERIOD`] without it.
pub const ENV_TELEMETRY_PERIOD_MS: &str = "SPINDLE_TELEMETRY_PERIOD_MS";
/// The sample period when none is set.
pub const DEFAULT_PERIOD: Duration = Duration::from_secs(1);
/// The vsock context id of the host, as a guest addresses it.
pub const VSOCK_HOST_CID: u32 = 2;

/// A collector's address: `unix:<path>` or `vsock:<cid>:<port>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    /// A Unix stream socket: the process target.
    Unix(PathBuf),
    /// A vsock stream socket: the QEMU and Firecracker targets, where the host is
    /// [`VSOCK_HOST_CID`].
    Vsock {
        /// The context id to connect to.
        cid: u32,
        /// The port.
        port: u32,
    },
}

impl Endpoint {
    /// The host's port `port`, as a guest reaches it.
    pub fn host_vsock(port: u32) -> Self {
        Self::Vsock {
            cid: VSOCK_HOST_CID,
            port,
        }
    }

    /// Connects, with writes that give up after `write_timeout`.
    pub fn connect(&self, write_timeout: Duration) -> io::Result<Connection> {
        match self {
            Self::Unix(path) => {
                let stream = UnixStream::connect(path)?;
                stream.set_write_timeout(Some(write_timeout))?;
                Ok(Connection::Unix(stream))
            }
            Self::Vsock { cid, port } => {
                let stream = VsockStream::connect(*cid, *port)?;
                stream.set_write_timeout(Some(write_timeout))?;
                Ok(Connection::Vsock(stream))
            }
        }
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unix(path) => write!(formatter, "unix:{}", path.display()),
            Self::Vsock { cid, port } => write!(formatter, "vsock:{cid}:{port}"),
        }
    }
}

/// Why a `SPINDLE_TELEMETRY` value is not an endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointError(pub String);

impl fmt::Display for EndpointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for EndpointError {}

impl FromStr for Endpoint {
    type Err = EndpointError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let refuse = || {
            EndpointError(format!(
                "{text:?} is not `unix:<path>` or `vsock:<cid>:<port>`"
            ))
        };
        if let Some(path) = text.strip_prefix("unix:") {
            return if path.is_empty() {
                Err(refuse())
            } else {
                Ok(Self::Unix(PathBuf::from(path)))
            };
        }
        let rest = text.strip_prefix("vsock:").ok_or_else(refuse)?;
        let (cid, port) = rest.split_once(':').ok_or_else(refuse)?;
        Ok(Self::Vsock {
            cid: cid.parse().map_err(|_| refuse())?,
            port: port.parse().map_err(|_| refuse())?,
        })
    }
}

/// An open connection to a collector.
#[derive(Debug)]
pub enum Connection {
    /// Over a Unix socket.
    Unix(UnixStream),
    /// Over vsock.
    Vsock(VsockStream),
}

impl Write for Connection {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            Self::Unix(stream) => stream.write(buffer),
            Self::Vsock(stream) => stream.write(buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
