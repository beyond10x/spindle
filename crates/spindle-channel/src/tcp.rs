//! A channel over TCP: each frame is a 4-byte big-endian length followed by that many bytes of
//! JSON.
//!
//! What ends the channel, as [`Channel`] states: the peer closing it ([`ChannelError::Closed`]),
//! a send that times out or fails, which may have left part of a frame on the stream, and a
//! received frame over [`MAX_FRAME`], after which the stream cannot be read in step. The first
//! two return `Broken` at once, the last `FrameTooLarge` once; every later call returns `Broken`.
//! A frame whose JSON does not decode is refused alone and the next frame is read.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::channel::{Channel, ChannelError};
use crate::model::Frame;

/// The largest frame either end sends or accepts.
pub const MAX_FRAME: usize = 16 * 1024 * 1024;

/// How long one send may take, its whole frame written, unless configured.
pub const DEFAULT_WRITE_TIMEOUT: Duration = Duration::from_secs(5);

const PREFIX: usize = 4;
const SMALLEST_TIMEOUT: Duration = Duration::from_millis(1);

/// One end of a TCP link.
pub struct TcpChannel {
    stream: TcpStream,
    buffer: Vec<u8>,
    broken: Option<String>,
    write_timeout: Duration,
}

impl TcpChannel {
    /// Connects to a listening end.
    pub fn connect(address: impl ToSocketAddrs) -> Result<Self, ChannelError> {
        Self::new(TcpStream::connect(address)?)
    }

    /// Wraps a connected stream, such as one a listener accepted.
    pub fn new(stream: TcpStream) -> Result<Self, ChannelError> {
        stream.set_nodelay(true)?;
        Ok(Self {
            stream,
            buffer: Vec::new(),
            broken: None,
            write_timeout: DEFAULT_WRITE_TIMEOUT,
        })
    }

    /// How long one send may take before it fails and breaks the channel. The bound is on the
    /// whole frame, not on each write: a peer that drains a few bytes at a time does not extend
    /// it. `Duration::MAX` sends without a bound.
    #[must_use]
    pub fn with_write_timeout(mut self, timeout: Duration) -> Self {
        self.write_timeout = timeout;
        self
    }

    fn write_frame(&mut self, bytes: &[u8]) -> Result<(), ChannelError> {
        let deadline = Instant::now().checked_add(self.write_timeout);
        self.stream.set_nonblocking(false)?;
        let mut written = 0;
        while written < bytes.len() {
            let timeout = match deadline {
                None => None,
                Some(deadline) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        return Err(self.break_with(format!(
                            "a send did not finish within {:?}, {written} of {} bytes written",
                            self.write_timeout,
                            bytes.len()
                        )));
                    }
                    Some(remaining.max(SMALLEST_TIMEOUT))
                }
            };
            self.stream.set_write_timeout(timeout)?;
            match self.stream.write(&bytes[written..]) {
                Ok(0) => {
                    return Err(self.break_with("the stream accepts no more bytes".to_owned()));
                }
                Ok(count) => written += count,
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(error) if is_closed(error.kind()) => {
                    self.broken = Some("the peer closed the connection".to_owned());
                    return Err(ChannelError::Closed);
                }
                Err(error) => {
                    return Err(self.break_with(format!(
                        "a send failed after {written} of {} bytes: {error}",
                        bytes.len()
                    )));
                }
            }
        }
        Ok(())
    }

    fn break_with(&mut self, reason: String) -> ChannelError {
        self.broken = Some(reason.clone());
        ChannelError::Broken(reason)
    }

    fn intact(&self) -> Result<(), ChannelError> {
        match &self.broken {
            Some(reason) => Err(ChannelError::Broken(reason.clone())),
            None => Ok(()),
        }
    }

    fn take(&mut self) -> Result<Option<Frame>, ChannelError> {
        let Some(prefix) = self.buffer.first_chunk::<PREFIX>() else {
            return Ok(None);
        };
        let size = u32::from_be_bytes(*prefix) as usize;
        if size > MAX_FRAME {
            self.broken = Some(format!(
                "received a frame of {size} bytes, over the limit of {MAX_FRAME}"
            ));
            self.buffer.clear();
            return Err(ChannelError::FrameTooLarge {
                size,
                limit: MAX_FRAME,
            });
        }
        if self.buffer.len() < PREFIX + size {
            return Ok(None);
        }
        let frame = serde_json::from_slice(&self.buffer[PREFIX..PREFIX + size])
            .map_err(|error| ChannelError::Frame(error.to_string()));
        self.buffer.drain(..PREFIX + size);
        frame.map(Some)
    }
}

impl Channel for TcpChannel {
    fn send(&mut self, frame: Frame) -> Result<(), ChannelError> {
        self.intact()?;
        let body =
            serde_json::to_vec(&frame).map_err(|error| ChannelError::Frame(error.to_string()))?;
        let size = u32::try_from(body.len())
            .ok()
            .filter(|size| *size as usize <= MAX_FRAME)
            .ok_or(ChannelError::FrameTooLarge {
                size: body.len(),
                limit: MAX_FRAME,
            })?;
        let mut bytes = Vec::with_capacity(PREFIX + body.len());
        bytes.extend_from_slice(&size.to_be_bytes());
        bytes.extend_from_slice(&body);
        self.write_frame(&bytes)
    }

    fn receive(&mut self, wait: Duration) -> Result<Option<Frame>, ChannelError> {
        self.intact()?;
        let deadline = Instant::now().checked_add(wait);
        let mut chunk = [0_u8; 8192];
        loop {
            if let Some(frame) = self.take()? {
                return Ok(Some(frame));
            }
            match deadline {
                None => {
                    self.stream.set_nonblocking(false)?;
                    self.stream.set_read_timeout(None)?;
                }
                Some(deadline) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        self.stream.set_nonblocking(true)?;
                    } else {
                        self.stream.set_nonblocking(false)?;
                        self.stream.set_read_timeout(Some(remaining))?;
                    }
                }
            }
            match self.stream.read(&mut chunk) {
                Ok(0) => {
                    self.broken = Some("the peer closed the connection".to_owned());
                    return Err(ChannelError::Closed);
                }
                Ok(read) => self.buffer.extend_from_slice(&chunk[..read]),
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
                {
                    return Ok(None);
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) if is_closed(error.kind()) => {
                    self.broken = Some("the peer closed the connection".to_owned());
                    return Err(ChannelError::Closed);
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
}

fn is_closed(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::BrokenPipe | ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted
    )
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use super::*;
    use crate::model::DeliveryId;

    fn linked() -> (TcpStream, TcpChannel) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let raw = TcpStream::connect(listener.local_addr().expect("bound")).expect("connects");
        let (accepted, _) = listener.accept().expect("accepts");
        (raw, TcpChannel::new(accepted).expect("a channel"))
    }

    #[test]
    fn a_frame_split_across_writes_is_reassembled() {
        let (mut raw, mut channel) = linked();
        let body = serde_json::to_vec(&Frame::Acknowledge(DeliveryId(9))).expect("encodes");
        let mut bytes = u32::try_from(body.len())
            .expect("small")
            .to_be_bytes()
            .to_vec();
        bytes.extend_from_slice(&body);
        let (head, tail) = bytes.split_at(3);
        raw.write_all(head).expect("writes");
        raw.flush().expect("flushes");
        assert!(
            channel
                .receive(Duration::from_millis(50))
                .expect("open")
                .is_none()
        );
        raw.write_all(tail).expect("writes");
        assert_eq!(
            channel.receive(Duration::from_secs(5)).expect("open"),
            Some(Frame::Acknowledge(DeliveryId(9)))
        );
    }

    #[test]
    fn a_frame_over_the_limit_is_refused() {
        let (mut raw, mut channel) = linked();
        let size = u32::try_from(MAX_FRAME + 1).expect("fits");
        raw.write_all(&size.to_be_bytes()).expect("writes");
        assert!(matches!(
            channel.receive(Duration::from_secs(5)),
            Err(ChannelError::FrameTooLarge { .. })
        ));
    }

    #[test]
    fn a_channel_that_refused_an_oversized_frame_stays_broken() {
        let (mut raw, mut channel) = linked();
        let size = u32::try_from(MAX_FRAME + 1).expect("fits");
        raw.write_all(&size.to_be_bytes()).expect("writes");
        assert!(matches!(
            channel.receive(Duration::from_secs(5)),
            Err(ChannelError::FrameTooLarge { .. })
        ));
        let body = serde_json::to_vec(&Frame::Acknowledge(DeliveryId(1))).expect("encodes");
        let mut bytes = u32::try_from(body.len())
            .expect("small")
            .to_be_bytes()
            .to_vec();
        bytes.extend_from_slice(&body);
        raw.write_all(&bytes).expect("writes");
        for _ in 0..3 {
            assert!(matches!(
                channel.receive(Duration::from_millis(50)),
                Err(ChannelError::Broken(_))
            ));
        }
        assert!(matches!(
            channel.send(Frame::Acknowledge(DeliveryId(2))),
            Err(ChannelError::Broken(_))
        ));
    }

    #[test]
    fn a_send_that_times_out_breaks_the_channel() {
        let (_stalled, channel) = linked();
        let mut channel = channel.with_write_timeout(Duration::from_millis(100));
        let body = serde_json::json!({ "body": "x".repeat(1024 * 1024) });
        let frame = Frame::Deliver(crate::model::Envelope {
            id: DeliveryId(1),
            binding: "b".to_owned(),
            from: "f".to_owned(),
            to: "t".to_owned(),
            delivery: ess_domain::binding::Delivery::AtMostOnce,
            on_failure: ess_domain::binding::Failure::Drop,
            attempt: 1,
            command: crate::model::Command {
                name: "c".to_owned(),
                fields: body.as_object().expect("an object").clone(),
            },
        });
        let failed = (0..256)
            .find_map(|_| channel.send(frame.clone()).err())
            .expect("a peer that reads nothing fills the buffers");
        assert!(matches!(failed, ChannelError::Broken(_)), "{failed:?}");
        assert!(matches!(
            channel.send(Frame::Acknowledge(DeliveryId(2))),
            Err(ChannelError::Broken(_))
        ));
        assert!(matches!(
            channel.receive(Duration::ZERO),
            Err(ChannelError::Broken(_))
        ));
    }

    #[test]
    fn a_closed_peer_is_reported() {
        let (raw, mut channel) = linked();
        drop(raw);
        assert!(matches!(
            channel.receive(Duration::from_secs(5)),
            Err(ChannelError::Closed)
        ));
    }
}
