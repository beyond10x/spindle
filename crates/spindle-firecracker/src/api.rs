//! The few `PUT` requests a launch makes on Firecracker's API socket, as plain HTTP/1.1.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use crate::LaunchError;

const TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn put(socket: &Path, path: &str, body: &serde_json::Value) -> Result<(), LaunchError> {
    let request = format!("PUT {path}");
    let failed = |source| LaunchError::io(format!("{request} on the API socket"), source);
    let mut stream = UnixStream::connect(socket).map_err(failed)?;
    stream.set_read_timeout(Some(TIMEOUT)).map_err(failed)?;
    stream.set_write_timeout(Some(TIMEOUT)).map_err(failed)?;
    let body = body.to_string();
    write!(
        stream,
        "PUT {path} HTTP/1.1\r\nHost: localhost\r\nAccept: application/json\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .map_err(failed)?;

    let mut received = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(at) = received.windows(4).position(|window| window == b"\r\n\r\n") {
            break at + 4;
        }
        let read = stream.read(&mut chunk).map_err(failed)?;
        if read == 0 {
            return Err(LaunchError::Api {
                request,
                status: 0,
                body: "the connection closed before a response".to_string(),
            });
        }
        received.extend_from_slice(&chunk[..read]);
    };
    let head = String::from_utf8_lossy(&received[..head_end]).to_string();
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);
    let length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    while received.len() < head_end + length {
        let read = stream.read(&mut chunk).map_err(failed)?;
        if read == 0 {
            break;
        }
        received.extend_from_slice(&chunk[..read]);
    }
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(LaunchError::Api {
            request,
            status,
            body: String::from_utf8_lossy(&received[head_end..])
                .trim()
                .to_string(),
        })
    }
}
