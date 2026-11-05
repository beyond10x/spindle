//! vsock stream sockets: a guest connects to the host, and the host accepts.
//!
//! The standard library has no `AF_VSOCK`, so these wrap the system calls directly. Writes use
//! `MSG_NOSIGNAL`: a collector that went away is an error, never a `SIGPIPE`.

use std::io::{self, Read, Write};
use std::mem;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::{Duration, Instant};

use crate::sys::{check, connect_within, set_timeout, stream_socket};

/// Any context id, for a listener: `VMADDR_CID_ANY`.
pub const CID_ANY: u32 = u32::MAX;
/// Any port, for a listener that lets the kernel choose: `VMADDR_PORT_ANY`.
pub const PORT_ANY: u32 = u32::MAX;

fn address(cid: u32, port: u32) -> libc::sockaddr_vm {
    // SAFETY: sockaddr_vm is plain old data; all-zero is a valid value.
    let mut address: libc::sockaddr_vm = unsafe { mem::zeroed() };
    address.svm_family = libc::AF_VSOCK as libc::sa_family_t;
    address.svm_cid = cid;
    address.svm_port = port;
    address
}

fn socket() -> io::Result<OwnedFd> {
    stream_socket(libc::AF_VSOCK)
}

/// A connected vsock stream.
#[derive(Debug)]
pub struct VsockStream(OwnedFd);

impl VsockStream {
    /// Connects to `port` of the context `cid`, waiting at most `within`.
    pub fn connect_within(cid: u32, port: u32, within: Duration) -> io::Result<Self> {
        let fd = socket()?;
        let address = address(cid, port);
        // SAFETY: `address` is a valid sockaddr_vm of the length given.
        unsafe {
            connect_within(
                &fd,
                (&raw const address).cast(),
                mem::size_of::<libc::sockaddr_vm>() as libc::socklen_t,
                within,
            )
        }?;
        Ok(Self(fd))
    }

    /// Writes give up after `timeout`; `None` waits for as long as it takes.
    pub fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        set_timeout(self.0.as_raw_fd(), libc::SO_SNDTIMEO, timeout)
    }

    /// Reads give up after `timeout`; `None` waits for as long as it takes.
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        set_timeout(self.0.as_raw_fd(), libc::SO_RCVTIMEO, timeout)
    }
}

impl Read for VsockStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        loop {
            // SAFETY: `buffer` is valid for `buffer.len()` bytes of writing.
            let read = unsafe {
                libc::recv(
                    self.0.as_raw_fd(),
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                    0,
                )
            };
            if read >= 0 {
                return Ok(read.unsigned_abs());
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }
}

impl Write for VsockStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        loop {
            // SAFETY: `buffer` is valid for `buffer.len()` bytes of reading.
            let written = unsafe {
                libc::send(
                    self.0.as_raw_fd(),
                    buffer.as_ptr().cast(),
                    buffer.len(),
                    libc::MSG_NOSIGNAL,
                )
            };
            if written >= 0 {
                return Ok(written.unsigned_abs());
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A listening vsock socket on the host.
#[derive(Debug)]
pub struct VsockListener {
    fd: OwnedFd,
    port: u32,
}

impl VsockListener {
    /// Listens on `port` of every context id of this machine.
    pub fn bind(port: u32) -> io::Result<Self> {
        let fd = socket()?;
        let address = address(CID_ANY, port);
        // SAFETY: `address` is a valid sockaddr_vm of the length given.
        check(unsafe {
            libc::bind(
                fd.as_raw_fd(),
                (&raw const address).cast(),
                mem::size_of::<libc::sockaddr_vm>() as libc::socklen_t,
            )
        })?;
        // SAFETY: a plain listen(2) call on a bound socket.
        check(unsafe { libc::listen(fd.as_raw_fd(), 16) })?;
        // SAFETY: sockaddr_vm is plain old data; all-zero is a valid value.
        let mut bound: libc::sockaddr_vm = unsafe { mem::zeroed() };
        let mut length = mem::size_of::<libc::sockaddr_vm>() as libc::socklen_t;
        // SAFETY: `bound` and `length` are valid for writing for the duration of the call.
        check(unsafe {
            libc::getsockname(fd.as_raw_fd(), (&raw mut bound).cast(), &raw mut length)
        })?;
        Ok(Self {
            fd,
            port: bound.svm_port,
        })
    }

    /// Listens on a port the kernel chooses; [`Self::port`] says which.
    pub fn bind_any_port() -> io::Result<Self> {
        Self::bind(PORT_ANY)
    }

    /// The port it listens on.
    pub fn port(&self) -> u32 {
        self.port
    }

    /// Accepts one connection, waiting at most `within`.
    pub fn accept_within(&self, within: Duration) -> io::Result<VsockStream> {
        let deadline = Instant::now() + within;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let mut poll = libc::pollfd {
                fd: self.fd.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let millis = libc::c_int::try_from(left.as_millis()).unwrap_or(libc::c_int::MAX);
            // SAFETY: `poll` is one valid pollfd for the duration of the call.
            let ready = unsafe { libc::poll(&raw mut poll, 1, millis) };
            if ready < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if ready == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "no vsock connection on port {} within {within:?}",
                        self.port
                    ),
                ));
            }
            // SAFETY: a plain accept4(2) call; the peer address is not asked for.
            let fd = check(unsafe {
                libc::accept4(
                    self.fd.as_raw_fd(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    libc::SOCK_CLOEXEC,
                )
            })?;
            // SAFETY: the descriptor was just accepted and is owned by nothing else.
            return Ok(VsockStream(unsafe { OwnedFd::from_raw_fd(fd) }));
        }
    }
}
