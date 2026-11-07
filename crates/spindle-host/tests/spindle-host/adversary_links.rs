//! Adversary cases for the links between one-component hosts (`Host::component`): the framing of
//! `hello`, `frame`, `settled` and `end`, and the settling that waits on them.
//!
//! The far ends are raw TCP streams the case drives byte by byte, so a case can send what no
//! well-behaved peer sends. Every read has a timeout and every loop a step bound; a stand-in peer
//! thread ends when its stream closes or its deadline passes.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, HostError, ManualClock};
use tempfile::TempDir;

const TABLE: &str = "rally-table";
const WEST: &str = "rally-player-west";
const EAST: &str = "rally-player-east";
const STEPS: usize = 400;
const STEP: Duration = Duration::from_millis(5);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("not an object: {other}"),
    }
}

/// The host of `component` alone, on `laptop-processes.toml`, with a listener for its links.
fn host_of(component: &str) -> (Host, TcpListener, TempDir) {
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
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    (host, listener, state)
}

fn send(stream: &mut TcpStream, message: &Value) {
    let body = serde_json::to_vec(message).expect("encodes");
    let mut bytes = u32::try_from(body.len())
        .expect("small")
        .to_be_bytes()
        .to_vec();
    bytes.extend_from_slice(&body);
    stream.write_all(&bytes).expect("the write");
}

fn hello(from: &str, to: &str) -> Value {
    json!({"hello": {"from": from, "to": to}})
}

/// A channel frame carrying one `at_most_once` envelope, checked against the channel's own type.
fn envelope(id: u64, binding: &str, from: &str, to: &str, command: &str, fields: &Value) -> Value {
    let frame = json!({"deliver": {
        "id": id,
        "binding": binding,
        "from": from,
        "to": to,
        "delivery": "at_most_once",
        "on_failure": "drop",
        "attempt": 1,
        "command": {"name": command, "fields": fields},
    }});
    serde_json::from_value::<spindle_channel::Frame>(frame.clone()).expect("a channel frame");
    json!({ "frame": frame })
}

fn move_paddle(id: u64) -> Value {
    envelope(
        id,
        "west-moves",
        WEST,
        TABLE,
        "rally.table.MovePaddle",
        &json!({"match_id": "rally", "side": "West", "step": "Up"}),
    )
}

enum Read1 {
    Message(Value),
    Quiet,
    Closed,
}

/// One whole message, waiting at most `within` for its first byte.
fn read_message(stream: &mut TcpStream, within: Duration) -> Read1 {
    stream
        .set_read_timeout(Some(within.max(Duration::from_millis(1))))
        .expect("a read timeout");
    let mut prefix = [0_u8; 4];
    match stream.read(&mut prefix[..1]) {
        Ok(0) => return Read1::Closed,
        Ok(_) => {}
        Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
            return Read1::Quiet;
        }
        Err(_) => return Read1::Closed,
    }
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("a read timeout");
    if stream.read_exact(&mut prefix[1..]).is_err() {
        return Read1::Closed;
    }
    let mut body = vec![0_u8; u32::from_be_bytes(prefix) as usize];
    if stream.read_exact(&mut body).is_err() {
        return Read1::Closed;
    }
    Read1::Message(serde_json::from_slice(&body).expect("the host sends JSON"))
}

/// Calls `accept` until `done` holds for the host or the step bound is spent.
fn accept_until(host: &mut Host, listener: &TcpListener, done: impl Fn(&mut Host) -> bool) {
    for _ in 0..STEPS {
        host.accept(listener).expect("the listener works");
        if done(host) {
            return;
        }
        std::thread::sleep(STEP);
    }
}

/// Accepts until `wanted` refusals are recorded or a deadline of 10 s passes, then a few more
/// steps so that a refusal beyond `wanted` still shows; returns the refusals and what is still
/// unlinked.
fn accept_for_refusals(
    host: &mut Host,
    listener: &TcpListener,
    wanted: usize,
) -> (Vec<String>, Vec<String>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut refused = Vec::new();
    while refused.len() < wanted && Instant::now() < deadline {
        host.accept(listener).expect("the listener works");
        refused.extend(host.refused());
        std::thread::sleep(STEP);
    }
    for _ in 0..10 {
        host.accept(listener).expect("the listener works");
        refused.extend(host.refused());
        std::thread::sleep(STEP);
    }
    (refused, host.unlinked())
}

/// What a stand-in player does with each envelope the table sends it.
#[derive(Clone, Copy)]
enum Player {
    Settles,
    Silent,
}

/// Plays a stand-in player on `stream` (the table's outgoing link to it) until the stream closes
/// or 30 s pass; returns every message it read.
fn stand_in(mut stream: TcpStream, player: Player) -> JoinHandle<Vec<Value>> {
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut seen = Vec::new();
        while Instant::now() < deadline {
            match read_message(&mut stream, Duration::from_millis(50)) {
                Read1::Message(message) => {
                    let deliver = message
                        .get("frame")
                        .and_then(|frame| frame.get("deliver"))
                        .is_some();
                    if deliver && matches!(player, Player::Settles) {
                        send(&mut stream, &json!({"settled": 1}));
                    }
                    let end = message.get("end").is_some();
                    seen.push(message);
                    if end {
                        break;
                    }
                }
                Read1::Quiet => {}
                Read1::Closed => break,
            }
        }
        seen
    })
}

/// The table, linked both ways to two stand-in players.
struct Linked {
    table: Host,
    _listener: TcpListener,
    _state: TempDir,
    west_out: Option<TcpStream>,
    east_out: Option<TcpStream>,
    /// The streams each stand-in opened to the table.
    west_in: TcpStream,
    _east_in: TcpStream,
}

impl Linked {
    fn new() -> Self {
        let (mut table, listener, state) = host_of(TABLE);
        let west = TcpListener::bind("127.0.0.1:0").expect("a stand-in west");
        let east = TcpListener::bind("127.0.0.1:0").expect("a stand-in east");
        table
            .connect(WEST, west.local_addr().expect("bound"))
            .expect("west listens");
        table
            .connect(EAST, east.local_addr().expect("bound"))
            .expect("east listens");
        let (mut west_out, _) = west.accept().expect("the table connects to west");
        let (mut east_out, _) = east.accept().expect("the table connects to east");
        for (stream, to) in [(&mut west_out, WEST), (&mut east_out, EAST)] {
            match read_message(stream, Duration::from_secs(5)) {
                Read1::Message(message) => assert_eq!(message, hello(TABLE, to)),
                _ => panic!("the table said no hello to {to}"),
            }
        }
        let mut west_in = TcpStream::connect(listener.local_addr().expect("bound")).expect("west");
        let mut east_in = TcpStream::connect(listener.local_addr().expect("bound")).expect("east");
        send(&mut west_in, &hello(WEST, TABLE));
        send(&mut east_in, &hello(EAST, TABLE));
        accept_until(&mut table, &listener, |host| host.unlinked().is_empty());
        assert_eq!(table.unlinked(), Vec::<String>::new(), "every link opens");
        Self {
            table,
            _listener: listener,
            _state: state,
            west_out: Some(west_out),
            east_out: Some(east_out),
            west_in,
            _east_in: east_in,
        }
    }

    fn players(&mut self, west: Player, east: Player) -> [JoinHandle<Vec<Value>>; 2] {
        [
            stand_in(self.west_out.take().expect("west, once"), west),
            stand_in(self.east_out.take().expect("east, once"), east),
        ]
    }

    fn open(&mut self) -> Result<(), HostError> {
        self.table
            .submit(
                "rally.table.OpenMatch",
                object(json!({"match_id": "rally", "target_score": 1})),
            )
            .map(|_| ())
    }
}

/// Delivers a while; the first error, if any.
fn deliver_for_a_while(host: &mut Host) -> Option<HostError> {
    for _ in 0..60 {
        if let Err(error) = host.deliver() {
            return Some(error);
        }
        std::thread::sleep(STEP);
    }
    None
}

#[test]
fn a_second_hello_before_the_link_opens_is_refused() {
    let (mut table, listener, _state) = host_of(TABLE);
    let mut stream = TcpStream::connect(listener.local_addr().expect("bound")).expect("connects");
    send(&mut stream, &hello(WEST, TABLE));
    send(&mut stream, &hello(EAST, TABLE));
    let (refused, unlinked) = accept_for_refusals(&mut table, &listener, 1);
    assert!(
        !refused.is_empty() && unlinked.contains(&format!("{EAST} (from)")),
        "one connection said hello as {WEST}, then as {EAST}; the table refused {refused:?} and \
         still waits for {unlinked:?}: the second hello was taken as the link's identity"
    );
}

#[test]
fn a_second_hello_on_an_open_link_is_refused_naming_the_peer() {
    let (mut table, listener, _state) = host_of(TABLE);
    let mut stream = TcpStream::connect(listener.local_addr().expect("bound")).expect("connects");
    send(&mut stream, &hello(WEST, TABLE));
    accept_until(&mut table, &listener, |host| {
        !host.unlinked().contains(&format!("{WEST} (from)"))
    });
    assert!(
        !table.unlinked().contains(&format!("{WEST} (from)")),
        "{WEST} links"
    );
    send(&mut stream, &hello(EAST, TABLE));
    let error = deliver_for_a_while(&mut table);
    assert!(
        matches!(&error, Some(HostError::Peer { peer, .. }) if peer == WEST),
        "an open link from {WEST} said hello again, as {EAST}; deliver answered {error:?}"
    );
}

#[test]
fn a_frame_before_hello_is_refused() {
    let (mut table, listener, _state) = host_of(TABLE);
    let mut stream = TcpStream::connect(listener.local_addr().expect("bound")).expect("connects");
    send(&mut stream, &move_paddle(1));
    send(&mut stream, &hello(WEST, TABLE));
    let (refused, unlinked) = accept_for_refusals(&mut table, &listener, 1);
    assert!(
        !refused.is_empty() && unlinked.contains(&format!("{WEST} (from)")),
        "a connection sent an envelope and only then said hello; the table refused {refused:?} \
         and still waits for {unlinked:?}: the envelope sent before hello rides on the link"
    );
}

#[test]
fn garbage_before_hello_is_refused_by_name() {
    let (mut table, listener, _state) = host_of(TABLE);
    let address = listener.local_addr().expect("bound");
    let mut garbage = TcpStream::connect(address).expect("connects");
    garbage
        .write_all(&[0, 0, 0, 5, b'{', b'n', b'o', b'p', b'e'])
        .expect("the write");
    let mut huge = TcpStream::connect(address).expect("connects");
    huge.write_all(&u32::MAX.to_be_bytes()).expect("the write");
    let (refused, _) = accept_for_refusals(&mut table, &listener, 2);
    assert_eq!(
        refused.len(),
        2,
        "one connection sent a message that is not JSON and one a length of {} bytes, each \
         instead of hello; the table refused {refused:?}",
        u32::MAX
    );
}

#[test]
fn end_from_a_player_does_not_end_the_table() {
    let mut linked = Linked::new();
    let players = linked.players(Player::Settles, Player::Settles);
    linked.open().expect("the match opens");
    send(&mut linked.west_in, &json!({"end": true}));
    let error = deliver_for_a_while(&mut linked.table);
    let ended = linked.table.ended();
    drop(linked);
    for player in players {
        let _ = player.join();
    }
    assert!(
        !ended,
        "{WEST} said end on its link to the table before the match had a winner; the table \
         takes the run as ended (deliver answered {error:?})"
    );
}

#[test]
fn a_frame_on_the_wrong_side_of_a_link_is_refused_naming_the_peer() {
    let mut linked = Linked::new();
    let mut west_out = linked.west_out.take().expect("west");
    send(&mut west_out, &move_paddle(1));
    let error = deliver_for_a_while(&mut linked.table);
    assert!(
        matches!(&error, Some(HostError::Peer { peer, .. }) if peer == WEST),
        "{WEST} sent an envelope back up the link the table opened to it, where only \
         acknowledgements and settled travel; deliver answered {error:?}, and the frame is \
         queued on the link where nothing ever reads it"
    );
}

#[test]
fn a_settled_nobody_owes_does_not_settle_the_next_envelope() {
    let mut linked = Linked::new();
    let mut west_out = linked.west_out.take().expect("west");
    send(&mut west_out, &json!({"settled": 1}));
    let east = stand_in(linked.east_out.take().expect("east"), Player::Settles);
    let west = stand_in(west_out, Player::Silent);
    let started = Instant::now();
    let opened = linked.open();
    let took = started.elapsed();
    drop(linked);
    let _ = (west.join(), east.join());
    assert!(
        opened.is_err(),
        "{WEST} sent settled before the table had sent it anything and never settled the \
         envelope OpenMatch sent it; OpenMatch still returned {opened:?} after {took:?}, before \
         what it led to on {WEST} settled"
    );
}

#[test]
fn the_settle_bound_names_the_one_peer_that_did_not_settle() {
    let mut linked = Linked::new();
    let players = linked.players(Player::Settles, Player::Silent);
    let opened = linked.open();
    drop(linked);
    for player in players {
        let _ = player.join();
    }
    assert!(
        matches!(&opened, Err(HostError::Peer { peer, .. }) if peer == EAST),
        "only {EAST} never settled; OpenMatch answered {opened:?}"
    );
}

#[test]
fn a_hello_naming_the_table_as_its_own_peer_is_refused() {
    let (mut table, listener, _state) = host_of(TABLE);
    let address = listener.local_addr().expect("bound");
    let mut own = TcpStream::connect(address).expect("connects");
    send(&mut own, &hello(TABLE, TABLE));
    let mut wrong = TcpStream::connect(address).expect("connects");
    send(&mut wrong, &hello(WEST, EAST));
    let mut first = TcpStream::connect(address).expect("connects");
    send(&mut first, &hello(EAST, TABLE));
    accept_until(&mut table, &listener, |host| {
        !host.unlinked().contains(&format!("{EAST} (from)"))
    });
    let mut second = TcpStream::connect(address).expect("connects");
    send(&mut second, &hello(EAST, TABLE));
    let (refused, unlinked) = accept_for_refusals(&mut table, &listener, 3);
    assert_eq!(refused.len(), 3, "{refused:?}");
    assert!(
        refused
            .iter()
            .any(|refusal| refusal.contains(TABLE) && refusal.contains("do not lead"))
    );
    assert!(
        refused
            .iter()
            .any(|refusal| refusal.contains(EAST) && refusal.contains("not"))
    );
    assert!(
        refused
            .iter()
            .any(|refusal| refusal.contains("second link"))
    );
    assert!(unlinked.contains(&format!("{WEST} (from)")), "{unlinked:?}");
}

#[test]
fn a_broken_frame_on_an_open_link_names_the_peer() {
    for bytes in [
        u32::MAX.to_be_bytes().to_vec(),
        vec![0, 0, 0, 5, b'{', b'n', b'o', b'p', b'e'],
        vec![0, 0, 0, 2, b'[', b']'],
    ] {
        let (mut table, listener, _state) = host_of(TABLE);
        let mut stream =
            TcpStream::connect(listener.local_addr().expect("bound")).expect("connects");
        send(&mut stream, &hello(WEST, TABLE));
        accept_until(&mut table, &listener, |host| {
            !host.unlinked().contains(&format!("{WEST} (from)"))
        });
        stream.write_all(&bytes).expect("the write");
        let error = deliver_for_a_while(&mut table);
        assert!(
            matches!(&error, Some(HostError::Peer { peer, .. }) if peer == WEST),
            "{bytes:?}: {error:?}"
        );
    }
}

#[test]
fn a_truncated_frame_then_close_names_the_peer() {
    let (mut table, listener, _state) = host_of(TABLE);
    let mut stream = TcpStream::connect(listener.local_addr().expect("bound")).expect("connects");
    send(&mut stream, &hello(WEST, TABLE));
    accept_until(&mut table, &listener, |host| {
        !host.unlinked().contains(&format!("{WEST} (from)"))
    });
    stream.write_all(&[0, 0, 0, 100, b'{']).expect("the write");
    drop(stream);
    let error = deliver_for_a_while(&mut table);
    assert!(
        matches!(&error, Some(HostError::Peer { peer, .. }) if peer == WEST),
        "{error:?}"
    );
}

/// A player running a command of its own (`submit`) when the table's envelope reaches it: the
/// envelope's consequences are still running when the player settles it.
#[test]
fn a_busy_player_settles_the_tables_envelope_only_after_what_it_led_to_settles() {
    let (mut west, listener, _state) = host_of(WEST);
    let table = TcpListener::bind("127.0.0.1:0").expect("a stand-in table");
    west.connect(TABLE, table.local_addr().expect("bound"))
        .expect("the table listens");
    let (mut west_to_table, _) = table.accept().expect("west connects");
    assert!(matches!(
        read_message(&mut west_to_table, Duration::from_secs(5)),
        Read1::Message(message) if message == hello(WEST, TABLE)
    ));
    let mut table_to_west =
        TcpStream::connect(listener.local_addr().expect("bound")).expect("connects");
    send(&mut table_to_west, &hello(TABLE, WEST));
    accept_until(&mut west, &listener, |host| host.unlinked().is_empty());
    assert_eq!(west.unlinked(), Vec::<String>::new());
    let seat = json!({"match_id": "rally", "ball_x": 5, "ball_y": 0, "paddle_y": 5});
    west.submit("rally.west.ObserveBall", object(seat.clone()))
        .expect("west takes its seat");

    let stand_in = std::thread::spawn(move || {
        let mut owed = 0_u32;
        let deadline = Instant::now() + Duration::from_secs(10);
        while owed == 0 && Instant::now() < deadline {
            if let Read1::Message(message) =
                read_message(&mut west_to_table, Duration::from_millis(50))
            {
                owed += u32::from(message.get("frame").is_some());
            }
        }
        send(
            &mut table_to_west,
            &envelope(
                1,
                "west-watches",
                TABLE,
                WEST,
                "rally.west.ObserveBall",
                &seat,
            ),
        );
        let mut early = None;
        let window = Instant::now() + Duration::from_secs(2);
        while Instant::now() < window {
            if let Read1::Message(message) =
                read_message(&mut west_to_table, Duration::from_millis(20))
            {
                owed += u32::from(message.get("frame").is_some());
            }
            if let Read1::Message(message) =
                read_message(&mut table_to_west, Duration::from_millis(20))
                && message.get("settled").is_some()
                && early.is_none()
            {
                early = Some(owed);
            }
        }
        for _ in 0..owed {
            send(&mut west_to_table, &json!({"settled": 1}));
        }
        let after = Instant::now() + Duration::from_secs(3);
        while Instant::now() < after {
            if matches!(
                read_message(&mut table_to_west, Duration::from_millis(50)),
                Read1::Closed
            ) {
                break;
            }
        }
        early
    });
    let submitted = west.submit(
        "rally.west.ObserveBall",
        object(json!({
            "match_id": "rally", "ball_x": 5, "ball_y": 0, "paddle_y": 5
        })),
    );
    let early = stand_in.join().expect("the stand-in table");
    drop(west);
    assert!(submitted.is_ok(), "{submitted:?}");
    assert_eq!(
        early,
        None,
        "west was running ObserveBall when the table's envelope came; it settled that envelope \
         while {} envelopes it sent the table were still unsettled, one of them the envelope's \
         own consequence",
        early.unwrap_or_default()
    );
}
