//! Adversary of wave 30 U8 (`story:peer-connect-test-steady`): `Host::connect` and `Host::link`
//! against a connection that reached itself, made the way a run makes one: a connect, through
//! `Host::connect` itself, to a loopback port that nothing listens on yet, repeated until the
//! kernel picks that same port as the source.

use std::io::Read;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustix::net::{
    AddressFamily, SocketType, bind, connect, getsockname, socket, sockopt::set_socket_linger,
};
use spindle_capability::Environment;
use spindle_host::{Host, HostError, ManualClock};
use tempfile::TempDir;

const TABLE: &str = "rally-table";
const WEST: &str = "rally-player-west";
/// How long a case tries to make `Host::connect` reach itself.
const REACH_ITSELF_WITHIN: Duration = Duration::from_secs(20);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace")
}

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

/// A loopback port nothing listens on, of the parity the kernel gives a connect's source port:
/// the source port of a connection to a stand-in, closed with a reset so no `TIME_WAIT` keeps it.
fn a_port_a_connect_leaves_from() -> u16 {
    let stand_in = TcpListener::bind("127.0.0.1:0").expect("a stand-in");
    let socket = socket(AddressFamily::INET, SocketType::STREAM, None).expect("a socket");
    connect(&socket, &stand_in.local_addr().expect("bound")).expect("a connection");
    let port = SocketAddr::try_from(getsockname(&socket).expect("its address"))
        .expect("IPv4")
        .port();
    set_socket_linger(&socket, Some(Duration::ZERO)).expect("a reset on close");
    drop(socket);
    drop(stand_in);
    port
}

/// Calls `Host::connect` to `127.0.0.1:port`, where nothing listens, until it returns anything
/// but "cannot be opened"; returns that result and the number of attempts.
fn connect_until_it_reaches_itself(host: &mut Host, port: u16) -> (Result<(), HostError>, usize) {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let deadline = Instant::now() + REACH_ITSELF_WITHIN;
    let mut attempts = 0;
    loop {
        attempts += 1;
        let result = host.connect(TABLE, address);
        let opened = !matches!(&result, Err(HostError::Peer { reason, .. }) if reason.contains("cannot be opened"));
        if opened {
            return (result, attempts);
        }
        assert!(
            Instant::now() < deadline,
            "no connect to {address} left from its own port in {attempts} attempts within \
             {REACH_ITSELF_WITHIN:?}"
        );
    }
}

/// Reads one frame from `stream`: a 4-byte big-endian length and that many bytes of JSON.
fn frame(mut stream: std::net::TcpStream) -> serde_json::Value {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("a read bound");
    let mut prefix = [0_u8; 4];
    stream.read_exact(&mut prefix).expect("a length");
    let mut body = vec![0_u8; u32::from_be_bytes(prefix) as usize];
    stream.read_exact(&mut body).expect("a body");
    serde_json::from_slice(&body).expect("JSON")
}

/// Acceptance 2 read literally: `Host::connect` itself refuses the connection it made to itself,
/// not only `Host::link` handed a stream built by hand.
#[test]
fn host_connect_refuses_the_connection_it_opened_to_itself() {
    let (mut host, _state) = host_of(WEST);
    let to_table = format!("{TABLE} (to)");
    let port = a_port_a_connect_leaves_from();
    let (result, attempts) = connect_until_it_reaches_itself(&mut host, port);
    let error = result.expect_err("a connection that reached itself is no link");
    assert!(
        matches!(&error, HostError::Peer { peer, reason } if peer == TABLE && reason.contains("reached itself")),
        "after {attempts} attempts: {error}"
    );
    assert!(
        host.unlinked().contains(&to_table),
        "a refused connection links nothing: {:?}",
        host.unlinked()
    );
}

/// The point of refusing: the caller tries again "until the peer listens". The peer, started
/// after the refusal, binds the port the refused connection reached, as `play --component` does
/// (`TcpListener::bind`), and the retry links it.
#[test]
fn the_peer_listens_at_the_port_a_refused_connection_reached() {
    let (mut host, _state) = host_of(WEST);
    let to_table = format!("{TABLE} (to)");
    let port = a_port_a_connect_leaves_from();
    let (result, attempts) = connect_until_it_reaches_itself(&mut host, port);
    let error = result.expect_err("a connection that reached itself is no link");
    assert!(
        matches!(&error, HostError::Peer { reason, .. } if reason.contains("reached itself")),
        "after {attempts} attempts: {error}"
    );
    let table = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).unwrap_or_else(|error| {
        panic!(
            "{TABLE}, started after {WEST}'s connection to 127.0.0.1:{port} reached itself and \
             was refused, cannot listen there: {error}"
        )
    });
    host.connect(TABLE, table.local_addr().expect("bound"))
        .expect("the retry reaches the table");
    assert!(!host.unlinked().contains(&to_table), "{TABLE} is linked");
    let (accepted, _) = table.accept().expect("the table accepts");
    assert_eq!(
        frame(accepted),
        serde_json::json!({"hello": {"from": WEST, "to": TABLE}})
    );
}

/// What holds the port in the case above: the refused stream, closed with a `FIN`, sits in
/// `TIME_WAIT` on its own address. The same stream closed with a reset (`SO_LINGER` 0, set here
/// before `Host::link` sees it) leaves the port free, so a peer can listen there at once.
#[test]
fn a_refused_connection_closed_with_a_reset_leaves_its_port_free() {
    let socket = socket(AddressFamily::INET, SocketType::STREAM, None).expect("a socket");
    bind(&socket, &SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).expect("a loopback port");
    let own = SocketAddr::try_from(getsockname(&socket).expect("its address")).expect("IPv4");
    connect(&socket, &own).expect("a connection to itself");
    set_socket_linger(&socket, Some(Duration::ZERO)).expect("a reset on close");
    let stream = std::net::TcpStream::from(socket);
    let (mut host, _state) = host_of(WEST);
    let error = host
        .link(TABLE, stream)
        .expect_err("a connection that reached itself is refused");
    assert!(error.to_string().contains("reached itself"), "{error}");
    TcpListener::bind(own).unwrap_or_else(|error| panic!("{own} after a reset: {error}"));
}

/// The premise of a fix: a socket connected to itself and closed with a reset alone, with no
/// `shutdown` first, leaves its port free for a listener at once.
#[test]
fn a_connection_to_itself_closed_with_a_reset_alone_frees_its_port() {
    let socket = socket(AddressFamily::INET, SocketType::STREAM, None).expect("a socket");
    bind(&socket, &SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).expect("a loopback port");
    let own = SocketAddr::try_from(getsockname(&socket).expect("its address")).expect("IPv4");
    connect(&socket, &own).expect("a connection to itself");
    set_socket_linger(&socket, Some(Duration::ZERO)).expect("a reset on close");
    drop(socket);
    TcpListener::bind(own).unwrap_or_else(|error| panic!("{own} after a reset: {error}"));
}

/// An IPv6 loopback socket connected to itself is refused as an IPv4 one is.
#[test]
fn an_ipv6_connection_that_reached_itself_is_no_link() {
    let socket = socket(AddressFamily::INET6, SocketType::STREAM, None).expect("a socket");
    if bind(&socket, &SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0)).is_err() {
        eprintln!("no IPv6 loopback on this host: nothing to check");
        return;
    }
    let own = SocketAddr::try_from(getsockname(&socket).expect("its address")).expect("IPv6");
    connect(&socket, &own).expect("a connection to itself");
    let stream = std::net::TcpStream::from(socket);
    assert_eq!(stream.local_addr().ok(), stream.peer_addr().ok());
    let (mut host, _state) = host_of(WEST);
    let error = host
        .link(TABLE, stream)
        .expect_err("an IPv6 connection that reached itself is refused");
    assert!(error.to_string().contains("reached itself"), "{error}");
}

/// An IPv4-mapped IPv6 socket connected to itself is refused as well.
#[test]
fn an_ipv4_mapped_connection_that_reached_itself_is_no_link() {
    let socket = socket(AddressFamily::INET6, SocketType::STREAM, None).expect("a socket");
    let mapped = Ipv4Addr::LOCALHOST.to_ipv6_mapped();
    if bind(&socket, &SocketAddrV6::new(mapped, 0, 0, 0)).is_err() {
        eprintln!("no dual-stack sockets on this host: nothing to check");
        return;
    }
    let own = SocketAddr::try_from(getsockname(&socket).expect("its address")).expect("IPv6");
    connect(&socket, &own).expect("a connection to itself");
    let stream = std::net::TcpStream::from(socket);
    assert_eq!(stream.local_addr().ok(), stream.peer_addr().ok());
    let (mut host, _state) = host_of(WEST);
    let error = host
        .link(TABLE, stream)
        .expect_err("an IPv4-mapped connection that reached itself is refused");
    assert!(error.to_string().contains("reached itself"), "{error}");
}

/// A peer listening on every IPv4 address is linked when reached at `0.0.0.0` and at its
/// loopback host: the check refuses no connection between two sockets.
#[test]
fn a_peer_on_the_unspecified_address_is_linked() {
    let table = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).expect("the table listens");
    let port = table.local_addr().expect("bound").port();
    for address in [
        SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port),
        SocketAddrV4::new(Ipv4Addr::new(127, 53, 1, 8), port),
    ] {
        let (mut host, _state) = host_of(WEST);
        host.connect(TABLE, SocketAddr::V4(address))
            .unwrap_or_else(|error| panic!("{address}: {error}"));
        let (accepted, _) = table.accept().expect("the table accepts");
        assert_eq!(
            frame(accepted),
            serde_json::json!({"hello": {"from": WEST, "to": TABLE}}),
            "{address}"
        );
    }
}
