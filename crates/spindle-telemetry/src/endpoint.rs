//! Where an instance sends its frames: the value of `SPINDLE_TELEMETRY`.

use std::fmt;
use std::io::{self, Write};
use std::mem;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use crate::sys::{connect_within, set_timeout, stream_socket};
use crate::vsock::VsockStream;

/// The variable a launcher gives an instance its endpoint in.
pub const ENV_TELEMETRY: &str = "SPINDLE_TELEMETRY";
/// The variable that sets the sample period, in milliseconds; [`DEFAULT_PERIOD`] without it.
pub const ENV_TELEMETRY_PERIOD_MS: &str = "SPINDLE_TELEMETRY_PERIOD_MS";
/// The sample period when none is set.
pub const DEFAULT_PERIOD: Duration = Duration::from_secs(1);
/// The vsock context id of the host, as a guest addresses it.
pub const VSOCK_HOST_CID: u32 = 2;
/// The longest Unix socket path, in bytes: `sun_path` less its last byte, the NUL. `sun_path`
/// holds 108 bytes on Linux (107) and 104 on macOS (103).
pub const MAX_UNIX_PATH: usize = {
    // SAFETY: sockaddr_un is plain old data; all-zero is a valid value.
    let address: libc::sockaddr_un = unsafe { mem::zeroed() };
    address.sun_path.len() - 1
};

/// A collector's address: `unix:<path>` or `vsock:<cid>:<port>`.
///
/// A Unix path is UTF-8 and at most [`MAX_UNIX_PATH`] bytes, so it reads back as written and the
/// kernel can connect to it; [`Endpoint::unix`] and parsing refuse any other, and
/// [`Self::check`] refuses one built from the variant directly.
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
    /// The Unix socket `path`; refused unless it is UTF-8 and at most [`MAX_UNIX_PATH`] bytes.
    pub fn unix(path: impl Into<PathBuf>) -> Result<Self, EndpointError> {
        let endpoint = Self::Unix(path.into());
        endpoint.check()?;
        Ok(endpoint)
    }

    /// The host's port `port`, as a guest reaches it.
    pub fn host_vsock(port: u32) -> Self {
        Self::Vsock {
            cid: VSOCK_HOST_CID,
            port,
        }
    }

    /// Whether the endpoint can be written down and connected to.
    pub fn check(&self) -> Result<(), EndpointError> {
        match self {
            Self::Unix(path) => unix_path(path).map(drop),
            Self::Vsock { .. } => Ok(()),
        }
    }

    /// Connects, waiting at most `within` for the connection and, once connected, at most
    /// `within` for each write.
    ///
    /// A collector that cannot take the connection now is an error at once: a Unix socket whose
    /// backlog is full refuses a non-blocking connect.
    pub fn connect(&self, within: Duration) -> io::Result<Connection> {
        match self {
            Self::Unix(path) => {
                let path = unix_path(path)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
                let fd = stream_socket(libc::AF_UNIX)?;
                // SAFETY: sockaddr_un is plain old data; all-zero is a valid value.
                let mut address: libc::sockaddr_un = unsafe { mem::zeroed() };
                address.sun_family = libc::AF_UNIX as libc::sa_family_t;
                for (slot, byte) in address.sun_path.iter_mut().zip(path.as_bytes()) {
                    *slot = *byte as libc::c_char;
                }
                // SAFETY: `address` is a sockaddr_un holding a NUL-terminated path.
                unsafe {
                    connect_within(
                        &fd,
                        (&raw const address).cast(),
                        mem::size_of::<libc::sockaddr_un>() as libc::socklen_t,
                        within,
                    )
                }?;
                set_timeout(fd.as_raw_fd(), libc::SO_SNDTIMEO, Some(within))?;
                Ok(Connection::Unix(UnixStream::from(fd)))
            }
            Self::Vsock { cid, port } => {
                let stream = VsockStream::connect_within(*cid, *port, within)?;
                stream.set_write_timeout(Some(within))?;
                Ok(Connection::Vsock(stream))
            }
        }
    }
}

fn unix_path(path: &Path) -> Result<&str, EndpointError> {
    let text = path.to_str().ok_or_else(|| {
        EndpointError(format!(
            "the Unix socket path {} is not UTF-8",
            path.display()
        ))
    })?;
    if text.is_empty() {
        return Err(EndpointError("the Unix socket path is empty".to_owned()));
    }
    if text.len() > MAX_UNIX_PATH {
        return Err(EndpointError(format!(
            "the Unix socket path {text} is {} bytes; sun_path holds at most {MAX_UNIX_PATH}",
            text.len()
        )));
    }
    if text.contains('\0') {
        return Err(EndpointError(format!(
            "the Unix socket path {text:?} holds a NUL"
        )));
    }
    Ok(text)
}

impl fmt::Display for Endpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unix(path) => write!(formatter, "unix:{}", path.display()),
            Self::Vsock { cid, port } => write!(formatter, "vsock:{cid}:{port}"),
        }
    }
}

/// Why a value is not an endpoint.
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
                Self::unix(path)
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

impl Connection {
    /// Each write gives up after `timeout`.
    pub fn set_write_timeout(&self, timeout: Duration) -> io::Result<()> {
        match self {
            Self::Unix(stream) => stream.set_write_timeout(Some(timeout)),
            Self::Vsock(stream) => stream.set_write_timeout(Some(timeout)),
        }
    }
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
