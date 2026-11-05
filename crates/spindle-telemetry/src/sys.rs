//! Connecting a stream socket within a bound time, which the standard library cannot do for a
//! Unix socket or at all for vsock.

use std::io;
use std::mem;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::time::{Duration, Instant};

pub(crate) fn check(result: libc::c_int) -> io::Result<libc::c_int> {
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(result)
    }
}

/// A new stream socket of `family`, close-on-exec.
pub(crate) fn stream_socket(family: libc::c_int) -> io::Result<OwnedFd> {
    // SAFETY: a plain socket(2) call.
    let fd = check(unsafe { libc::socket(family, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0) })?;
    // SAFETY: the descriptor was just created and is owned by nothing else.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn set_nonblocking(fd: RawFd, nonblocking: bool) -> io::Result<()> {
    // SAFETY: fcntl on a descriptor this process owns; no pointers.
    let flags = check(unsafe { libc::fcntl(fd, libc::F_GETFL) })?;
    let flags = if nonblocking {
        flags | libc::O_NONBLOCK
    } else {
        flags & !libc::O_NONBLOCK
    };
    // SAFETY: as above.
    check(unsafe { libc::fcntl(fd, libc::F_SETFL, flags) }).map(drop)
}

/// Sets `SO_SNDTIMEO` or `SO_RCVTIMEO`; `None` waits for as long as it takes.
pub(crate) fn set_timeout(
    fd: RawFd,
    option: libc::c_int,
    timeout: Option<Duration>,
) -> io::Result<()> {
    let timeout = timeout.unwrap_or(Duration::ZERO);
    // SAFETY: timeval is plain old data; all-zero is a valid value.
    let mut value: libc::timeval = unsafe { mem::zeroed() };
    value.tv_sec = timeout.as_secs().try_into().unwrap_or(i32::MAX.into());
    value.tv_usec = timeout.subsec_micros().into();
    // SAFETY: `value` is a valid timeval for the duration of the call.
    check(unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            option,
            (&raw const value).cast(),
            mem::size_of::<libc::timeval>() as libc::socklen_t,
        )
    })
    .map(drop)
}

/// Connects `fd` to `address` without waiting longer than `within`, then makes it blocking
/// again. A peer that cannot take the connection now (a full backlog) is an error at once.
///
/// # Safety
///
/// `address` points to a socket address of `length` bytes of the socket's family.
pub(crate) unsafe fn connect_within(
    fd: &OwnedFd,
    address: *const libc::sockaddr,
    length: libc::socklen_t,
    within: Duration,
) -> io::Result<()> {
    let raw = fd.as_raw_fd();
    set_nonblocking(raw, true)?;
    // SAFETY: the caller guarantees `address` and `length`.
    let result = unsafe { libc::connect(raw, address, length) };
    if result < 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::EINPROGRESS) {
            return Err(error);
        }
        let deadline = Instant::now() + within;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let mut poll = libc::pollfd {
                fd: raw,
                events: libc::POLLOUT,
                revents: 0,
            };
            let millis = libc::c_int::try_from(left.as_millis()).unwrap_or(libc::c_int::MAX);
            // SAFETY: `poll` is one valid pollfd for the duration of the call.
            let ready = unsafe { libc::poll(&raw mut poll, 1, millis) };
            if ready < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted && !left.is_zero() {
                    continue;
                }
                return Err(error);
            }
            if ready == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("not connected within {within:?}"),
                ));
            }
            break;
        }
        let mut error: libc::c_int = 0;
        let mut size = mem::size_of::<libc::c_int>() as libc::socklen_t;
        // SAFETY: `error` and `size` are valid for writing for the duration of the call.
        check(unsafe {
            libc::getsockopt(
                raw,
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                (&raw mut error).cast(),
                &raw mut size,
            )
        })?;
        if error != 0 {
            return Err(io::Error::from_raw_os_error(error));
        }
    }
    set_nonblocking(raw, false)
}
