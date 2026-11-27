//! `Host::connect` and `Host::link`: a connection that reached itself is no link.
//!
//! A connect to a loopback port that nothing listens on can leave from that same port, and Linux
//! then joins the socket to itself (a TCP simultaneous open). That stream says `hello` to itself,
//! and the run fails on a link no peer ever held. The case makes such a stream on purpose: a
//! socket bound to a loopback port and connected to that same address.

use std::io::Read;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rustix::net::{AddressFamily, SocketType, bind, connect, getsockname, socket};
use spindle_capability::Environment;
use spindle_host::{Host, HostError, ManualClock};
use tempfile::TempDir;

const TABLE: &str = "rally-table";
const WEST: &str = "rally-player-west";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace")
}

/// The host of `component` alone, on `laptop-processes.toml`.
fn host_of(component: &str) -> (Host, TempDir) {
    let loaded = spindle_spec::load(&root().join("examples/rally/ess")).expect("rally compiles");
    let text = std::fs::read_to_string(root().join("examples/environments/laptop-processes.toml"))
        .expect("laptop-processes.toml");
    let environment = Environment::parse(&text).expect("an environment");
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let host = Host::component(
        loaded.ir,
        &environment,
        Arc::new(ManualClock::new()),
        state.path(),
        component,
    )
    .expect("the component is admitted");
    (host, state)
}

/// A stream whose local and peer address are one: a socket on a loopback port, connected to
/// that same address while nothing listens there.
fn reached_itself() -> TcpStream {
    let socket = socket(AddressFamily::INET, SocketType::STREAM, None).expect("a socket");
    bind(&socket, &SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).expect("a loopback port");
    let own = SocketAddr::try_from(getsockname(&socket).expect("its address")).expect("IPv4");
    connect(&socket, &own).expect("a connection to itself");
    let stream = TcpStream::from(socket);
    assert_eq!(
        stream.local_addr().expect("local"),
        stream.peer_addr().expect("peer"),
        "the stream reached itself"
    );
    stream
}

#[test]
fn a_connection_that_reached_itself_is_no_link() {
    let (mut host, _state) = host_of(WEST);
    assert!(host.peers().contains_key(TABLE), "{WEST} sends to {TABLE}");
    let to_table = format!("{TABLE} (to)");

    let error = host
        .link(TABLE, reached_itself())
        .expect_err("a connection that reached itself is refused");
    assert!(
        matches!(&error, HostError::Peer { peer, reason } if peer == TABLE && reason.contains("reached itself")),
        "{error}"
    );
    assert!(
        host.unlinked().contains(&to_table),
        "a refused connection links nothing: {:?}",
        host.unlinked()
    );

    let table = TcpListener::bind("127.0.0.1:0").expect("the table listens");
    host.connect(TABLE, table.local_addr().expect("bound"))
        .expect("the retry reaches the table");
    assert!(
        !host.unlinked().contains(&to_table),
        "the retry links {TABLE}: {:?}",
        host.unlinked()
    );
    let (mut accepted, _) = table.accept().expect("the table accepts");
    accepted
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("a read bound");
    let mut prefix = [0_u8; 4];
    accepted.read_exact(&mut prefix).expect("a length");
    let mut body = vec![0_u8; u32::from_be_bytes(prefix) as usize];
    accepted.read_exact(&mut body).expect("a hello");
    let hello = serde_json::from_slice::<serde_json::Value>(&body).expect("JSON");
    assert_eq!(
        hello,
        serde_json::json!({"hello": {"from": WEST, "to": TABLE}}),
        "the table hears {WEST}'s hello, once"
    );
}
