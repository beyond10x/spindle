//! Adversary of wave 30 U8 (`story:peer-connect-test-steady`): `Host::connect` and `Host::link`
//! against a connection that reached itself, made the way a run makes one: a connect, through
//! `Host::connect` itself, to a loopback port that nothing listens on yet, repeated until the
//! kernel picks that same port as the source.
//!
//! A case that listens at the port a refused connection let go shares that port with every other
//! socket of the host: the kernel gives one source port to several connections whose destinations
//! differ, so another process's connection, or its `TIME_WAIT` for a minute after it, can hold the
//! port before the refused connection leaves from it and after. Such a case runs in rounds: when
//! its bind fails, it reads the kernel's TCP tables, fails if the refused connection holds the
//! port, and takes a new port only when another socket is seen holding it
//! ([`listen_where_it_reached`]).

use std::io::{ErrorKind, Read};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, TcpListener};
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
/// How long a case tries to make `Host::connect` reach itself, over all its rounds.
const REACH_ITSELF_WITHIN: Duration = Duration::from_secs(20);
/// How many rounds a case runs before it gives up finding a port no other socket takes.
const ROUNDS: usize = 10;

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

/// A TCP socket of this network namespace as `/proc/net/tcp` or `/proc/net/tcp6` lists it; `state`
/// is the kernel's state in hex (`06` is `TIME_WAIT`, `0A` listening).
#[derive(Debug)]
#[expect(dead_code, reason = "read through Debug in a failure message")]
struct Socket {
    local: SocketAddr,
    remote: SocketAddr,
    state: String,
}

/// An address as the kernel's TCP tables print it: the host as 32-bit words in hex, each in the
/// byte order of memory, and the port in hex.
fn table_address(text: &str) -> SocketAddr {
    let (host, port) = text.split_once(':').expect("host:port");
    let port = u16::from_str_radix(port, 16).expect("a port in hex");
    let bytes: Vec<u8> = (0..host.len())
        .step_by(8)
        .flat_map(|at| {
            u32::from_str_radix(&host[at..at + 8], 16)
                .expect("a word in hex")
                .to_ne_bytes()
        })
        .collect();
    let ip = match <[u8; 4]>::try_from(bytes.as_slice()) {
        Ok(v4) => IpAddr::from(v4),
        Err(_) => IpAddr::from(<[u8; 16]>::try_from(bytes.as_slice()).expect("16 bytes")),
    };
    SocketAddr::new(ip, port)
}

/// The sockets of this host that hold the port of `own` where a listener at `own` cannot bind:
/// those whose local and remote address are `own`, a connection that reached itself, and every
/// other socket at `own`'s port on `own`'s host or on the unspecified address.
fn holders(own: SocketAddr) -> (Vec<Socket>, Vec<Socket>) {
    let (mut itself, mut others) = (Vec::new(), Vec::new());
    for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
        let Ok(text) = std::fs::read_to_string(table) else {
            continue;
        };
        for line in text.lines().skip(1) {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let [_, local, remote, state, ..] = fields.as_slice() else {
                continue;
            };
            let (local, remote) = (table_address(local), table_address(remote));
            let local = SocketAddr::new(local.ip().to_canonical(), local.port());
            let remote = SocketAddr::new(remote.ip().to_canonical(), remote.port());
            if local.port() != own.port() {
                continue;
            }
            let socket = Socket {
                local,
                remote,
                state: (*state).to_owned(),
            };
            if local == own && remote == own {
                itself.push(socket);
            } else if local.ip() == own.ip() || local.ip().is_unspecified() {
                others.push(socket);
            }
        }
    }
    (itself, others)
}

/// Listens at `own`, the address a refused connection reached, as the peer does; `None` when
/// another socket of the host holds that port, as the kernel's tables read after the bind failed.
///
/// # Panics
/// When the bind fails and the refused connection still holds `own`, or no socket is seen holding
/// it, or it fails with anything but "address in use".
fn listen_where_it_reached(own: SocketAddr) -> Option<TcpListener> {
    let error = match TcpListener::bind(own) {
        Ok(listener) => return Some(listener),
        Err(error) => error,
    };
    let cannot = format!(
        "nothing can listen at {own}, where a connection reached itself and was closed: {error}"
    );
    assert_eq!(error.kind(), ErrorKind::AddrInUse, "{cannot}");
    let (itself, others) = holders(own);
    assert!(
        itself.is_empty(),
        "{cannot}; the refused connection holds it: {itself:?}"
    );
    assert!(!others.is_empty(), "{cannot}; no socket is seen holding it");
    eprintln!("{own} is held by another socket of this host, {others:?}: a new round");
    None
}

/// Runs `round` until it returns `true`, at most [`ROUNDS`] times; a round returns `false` only
/// when [`listen_where_it_reached`] saw another socket of the host holding its port.
fn in_rounds(mut round: impl FnMut() -> bool) {
    for _ in 0..ROUNDS {
        if round() {
            return;
        }
    }
    panic!("in each of {ROUNDS} rounds another socket of this host held the port first");
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
fn connect_until_it_reaches_itself(
    host: &mut Host,
    port: u16,
    deadline: Instant,
) -> (Result<(), HostError>, usize) {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
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
    let deadline = Instant::now() + REACH_ITSELF_WITHIN;
    let (result, attempts) = connect_until_it_reaches_itself(&mut host, port, deadline);
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
    let deadline = Instant::now() + REACH_ITSELF_WITHIN;
    in_rounds(|| {
        let (mut host, _state) = host_of(WEST);
        let to_table = format!("{TABLE} (to)");
        let port = a_port_a_connect_leaves_from();
        let (result, attempts) = connect_until_it_reaches_itself(&mut host, port, deadline);
        let error = result.expect_err("a connection that reached itself is no link");
        assert!(
            matches!(&error, HostError::Peer { reason, .. } if reason.contains("reached itself")),
            "after {attempts} attempts: {error}"
        );
        let Some(table) = listen_where_it_reached(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
        else {
            return false;
        };
        host.connect(TABLE, table.local_addr().expect("bound"))
            .expect("the retry reaches the table");
        assert!(!host.unlinked().contains(&to_table), "{TABLE} is linked");
        let (accepted, _) = table.accept().expect("the table accepts");
        assert_eq!(
            frame(accepted),
            serde_json::json!({"hello": {"from": WEST, "to": TABLE}})
        );
        true
    });
}

/// What holds the port in the case above: the refused stream, closed with a `FIN`, sits in
/// `TIME_WAIT` on its own address. The same stream closed with a reset (`SO_LINGER` 0, set here
/// before `Host::link` sees it) leaves the port free, so a peer can listen there at once.
#[test]
fn a_refused_connection_closed_with_a_reset_leaves_its_port_free() {
    in_rounds(|| {
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
        listen_where_it_reached(own).is_some()
    });
}

/// The premise of a fix: a socket connected to itself and closed with a reset alone, with no
/// `shutdown` first, leaves its port free for a listener at once.
#[test]
fn a_connection_to_itself_closed_with_a_reset_alone_frees_its_port() {
    in_rounds(|| {
        let socket = socket(AddressFamily::INET, SocketType::STREAM, None).expect("a socket");
        bind(&socket, &SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).expect("a loopback port");
        let own = SocketAddr::try_from(getsockname(&socket).expect("its address")).expect("IPv4");
        connect(&socket, &own).expect("a connection to itself");
        set_socket_linger(&socket, Some(Duration::ZERO)).expect("a reset on close");
        drop(socket);
        listen_where_it_reached(own).is_some()
    });
}

/// The rounds above cannot hide the refused connection: a connection to itself closed with a
/// `FIN`, in `TIME_WAIT` on its own address, keeps a listener out, and the kernel's tables show it
/// as that connection, which fails a case, not as another socket, which starts a new round.
#[test]
fn a_connection_to_itself_closed_with_a_fin_is_read_as_the_refused_one() {
    let socket = socket(AddressFamily::INET, SocketType::STREAM, None).expect("a socket");
    bind(&socket, &SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).expect("a loopback port");
    let own = SocketAddr::try_from(getsockname(&socket).expect("its address")).expect("IPv4");
    connect(&socket, &own).expect("a connection to itself");
    drop(socket);
    let error = TcpListener::bind(own).expect_err("a connection to itself closed with a FIN");
    assert_eq!(error.kind(), ErrorKind::AddrInUse, "{own}: {error}");
    let (itself, _) = holders(own);
    assert!(
        !itself.is_empty(),
        "the kernel's tables show no connection from {own} to itself"
    );
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
