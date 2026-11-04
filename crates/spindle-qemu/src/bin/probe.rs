//! `spindle-qemu-probe`: a guest-side probe of a microVM's network and resources.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};

/// How long one side of an exchange waits for the other before it fails.
const EXCHANGE: Duration = Duration::from_secs(30);

#[derive(Debug, Parser)]
#[command(name = "spindle-qemu-probe", about = "Probe a spindle microVM guest")]
struct Cli {
    #[command(subcommand)]
    command: Probe,
}

#[derive(Debug, Subcommand)]
enum Probe {
    /// Accept one TCP connection, answer its line with `pong <line>`, print `served <peer>`.
    Serve {
        /// The address to listen on.
        listen: SocketAddr,
    },
    /// Connect (retrying), send a line, print `reply: <answer>`.
    Call {
        /// The address to connect to.
        address: SocketAddr,
        /// The line to send.
        message: String,
        /// Give up connecting after this many seconds.
        #[arg(long, default_value_t = 60)]
        within: u64,
    },
    /// Print `cpus=<n>`, `mem_total_kib=<n>` and the launch plan's `instance=<id>`.
    Facts,
    /// Count one command, start the telemetry sender `SPINDLE_TELEMETRY` names, and exit once it
    /// has sent this many frames.
    Telemetry {
        /// The frames to send.
        #[arg(long, default_value_t = 3)]
        frames: u64,
        /// Give up after this many seconds.
        #[arg(long, default_value_t = 60)]
        within: u64,
    },
    /// Exit with this status.
    Exit {
        /// The exit status.
        code: u8,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Probe::Serve { listen } => report(serve(listen)),
        Probe::Call {
            address,
            message,
            within,
        } => report(call(address, &message, Duration::from_secs(within))),
        Probe::Facts => report(facts()),
        Probe::Telemetry { frames, within } => {
            report(telemetry(frames, Duration::from_secs(within)))
        }
        Probe::Exit { code } => ExitCode::from(code),
    }
}

fn report(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("spindle-qemu-probe: {error}");
            ExitCode::FAILURE
        }
    }
}

fn serve(listen: SocketAddr) -> Result<(), String> {
    let listener = TcpListener::bind(listen).map_err(|error| format!("bind {listen}: {error}"))?;
    println!("listening {listen}");
    let (stream, peer) = listener
        .accept()
        .map_err(|error| format!("accept: {error}"))?;
    stream
        .set_read_timeout(Some(EXCHANGE))
        .map_err(|error| format!("read timeout: {error}"))?;
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|error| format!("read: {error}"))?;
    writeln!(&stream, "pong {}", line.trim()).map_err(|error| format!("write: {error}"))?;
    stream
        .shutdown(Shutdown::Write)
        .map_err(|error| format!("shutdown: {error}"))?;
    // The guest powers off once this probe exits, and a segment it never sent or never
    // retransmitted would leave the caller waiting: return only when the caller has closed.
    let mut rest = Vec::new();
    reader
        .read_to_end(&mut rest)
        .map_err(|error| format!("waiting for {peer} to close: {error}"))?;
    println!("served {peer}");
    Ok(())
}

fn call(address: SocketAddr, message: &str, within: Duration) -> Result<(), String> {
    let deadline = Instant::now() + within;
    let stream = loop {
        match TcpStream::connect_timeout(&address, Duration::from_secs(1)) {
            Ok(stream) => break stream,
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(error) => return Err(format!("connect {address}: {error}")),
        }
    };
    stream
        .set_read_timeout(Some(EXCHANGE))
        .map_err(|error| format!("read timeout: {error}"))?;
    writeln!(&stream, "{message}").map_err(|error| format!("write: {error}"))?;
    let mut reply = String::new();
    BufReader::new(&stream)
        .read_line(&mut reply)
        .map_err(|error| format!("read: {error}"))?;
    println!("reply: {}", reply.trim());
    Ok(())
}

fn facts() -> Result<(), String> {
    let cpus = std::thread::available_parallelism()
        .map_err(|error| format!("cpus: {error}"))?
        .get();
    println!("cpus={cpus}");
    let meminfo = std::fs::read_to_string("/proc/meminfo")
        .map_err(|error| format!("/proc/meminfo: {error}"))?;
    let total = meminfo
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))
        .and_then(|rest| {
            rest.trim()
                .trim_end_matches("kB")
                .trim()
                .parse::<u64>()
                .ok()
        })
        .ok_or("/proc/meminfo has no MemTotal")?;
    println!("mem_total_kib={total}");
    let plan = std::fs::read_to_string("/etc/spindle/launch.json")
        .map_err(|error| format!("/etc/spindle/launch.json: {error}"))?;
    let plan: serde_json::Value =
        serde_json::from_str(&plan).map_err(|error| format!("launch.json: {error}"))?;
    println!(
        "instance={}",
        plan["instance_id"].as_str().unwrap_or_default()
    );
    Ok(())
}

fn telemetry(frames: u64, within: Duration) -> Result<(), String> {
    use spindle_telemetry::{Catalogue, Counter, Counters, Key, Kind, Outcome, Record, Source};

    let command = "probe.Ping";
    let counters = std::sync::Arc::new(Counters::new(Catalogue {
        component: "probe".to_owned(),
        counters: vec![Counter {
            name: format!("command.{command}.ok"),
            kind: Kind::Counter,
            source: Source::Kernel,
            key: Key::Outcome {
                command: command.to_owned(),
                outcome: "ok".to_owned(),
            },
        }],
    }));
    spindle_telemetry::Telemetry::record(
        counters.as_ref(),
        Record::Command {
            command,
            outcome: Outcome::Declared("ok"),
            elapsed: Duration::from_micros(5),
        },
    );
    let sender =
        spindle_telemetry::Sender::from_env(counters)?.ok_or("no SPINDLE_TELEMETRY is set")?;
    let sent = sender.wait_until_sent(frames, within);
    println!(
        "telemetry sent={} dropped={}",
        sender.sent(),
        sender.dropped()
    );
    if !sent {
        return Err(format!("{frames} frames were not sent within {within:?}"));
    }
    // The guest powers off when this probe exits; give the last frame time to leave it.
    std::thread::sleep(Duration::from_secs(1));
    sender.stop();
    Ok(())
}
