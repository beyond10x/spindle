//! The loopback listener answers the recorded exchanges: the review fixture served on 127.0.0.1
//! with the console routes, its external outcomes answered from the decisions file, every store in
//! memory. The Service Worker test holds its own answers to the same record, so the two answer
//! each request alike.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use spindle_capability::Environment;
use spindle_host::{Answers, Host, ManualClock};
use spindle_ingress::{HTTP_LOOPBACK, Listener, Surface};

use crate::Exchange;

/// How many exchanges one request may wait for before the test gives up.
const STEPS: usize = 50;
/// How long one wait for an exchange lasts.
const WAIT: Duration = Duration::from_millis(200);

fn loopback() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"loopback\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"memory\"]\n",
        "\"store.ephemeral\" = [\"memory\"]\n",
        "clock = [\"system\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
        "\"identity.mint\" = [\"uuid-v4\"]\n",
        "\"ingress.network\" = [\"http-loopback\"]\n",
        "\"external.*\" = [\"decisions-file\"]\n",
    ))
    .expect("the loopback environment parses")
}

/// One HTTP/1.1 request, `Connection: close`: the status, the `Allow` header and the body text.
fn send(address: SocketAddr, exchange: &Exchange) -> (u16, Option<String>, String) {
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a read timeout");
    let content_type = exchange
        .content_type
        .as_ref()
        .map(|value| format!("Content-Type: {value}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "{} {} HTTP/1.1\r\nHost: {address}\r\n{content_type}Content-Length: {}\r\n\
         Connection: close\r\n\r\n{}",
        exchange.method,
        exchange.target,
        exchange.body.len(),
        exchange.body
    )
    .expect("the request is written");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("the response is read");
    let (head, body) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("a response without a body: {response}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a response without a status: {head}"));
    let allow = head.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case("allow")
            .then(|| value.trim().to_owned())
    });
    (status, allow, body.to_owned())
}

#[test]
fn the_loopback_listener_answers_the_recorded_exchanges() {
    let ir = crate::ir();
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let answers = Answers::parse(&ir, crate::DECISIONS).expect("the decisions file reads");
    let mut host = Host::deciding(
        ir.clone(),
        &loopback(),
        Arc::new(ManualClock::new()),
        state.path(),
        answers.served(),
    )
    .unwrap_or_else(|error| panic!("the host admits the review fixture: {error}"));
    let surface = Surface::new(&ir, &["probe".to_owned()], None)
        .and_then(|surface| surface.with_console(HTTP_LOOPBACK))
        .unwrap_or_else(|refused| panic!("the review surface is served: {refused}"));
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();

    let recorded = crate::exchanges();
    let mut listened = Vec::new();
    for exchange in &recorded {
        let client = {
            let exchange = exchange.clone();
            thread::spawn(move || send(address, &exchange))
        };
        for _ in 0..STEPS {
            if client.is_finished() {
                break;
            }
            if let Some(served) = listener.next(WAIT) {
                served.serve(&mut host);
            }
        }
        assert!(
            client.is_finished(),
            "{} {} was not answered in time",
            exchange.method,
            exchange.target
        );
        let (status, allow, answer) = client.join().expect("the client thread");
        listened.push(Exchange {
            status,
            allow,
            answer,
            ..exchange.clone()
        });
    }
    assert_eq!(
        listened,
        recorded,
        "the listener answers what tests/fixtures/review/exchanges.json records; it wrote:\n{}",
        serde_json::to_string_pretty(&listened).unwrap_or_default()
    );
}
