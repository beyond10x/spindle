//! `/init` of a Spindle guest: a static binary the kernel starts as PID 1.
//!
//! It mounts `/proc`, `/sys` and `/dev`, reports the vCPUs, memory and `SPINDLE_*` variables it
//! sees, runs its command, prints `spindle-init: exit status <n>` and reboots, which ends the
//! Firecracker process (`reboot=k`). The network is already up: the kernel configures `eth0`
//! from `ip=` on its command line. Run as any other PID it exits with the status instead.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};

/// The status `/init` reports when it panicked, as Rust's runtime does for a panicking `main`.
const PANICKED: i32 = 101;

#[derive(Debug, Parser)]
#[command(name = "spindle-guest-init", about = "PID 1 of a Spindle guest")]
struct Cli {
    #[command(subcommand)]
    command: Mode,
}

#[derive(Debug, Subcommand)]
enum Mode {
    /// Run `/bin/b10x-spindle` with the remaining arguments.
    ///
    /// The kernel ends init's arguments at a second `--`, so none may follow `run`.
    Run {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Serve `SPINDLE_LISTEN` and greet every `SPINDLE_PEER_<COMPONENT>` over TCP; succeed once
    /// every peer answered and every peer was answered.
    Exchange {
        /// Seconds before giving up; a deadline past what the clock can represent is refused.
        #[arg(long, default_value_t = 60)]
        timeout: u64,
    },
}

fn main() {
    let pid1 = std::process::id() == 1;
    std::panic::set_hook(Box::new(move |panic| {
        say(&format!("panicked: {panic}"));
        say(&format!("exit status {PANICKED}"));
        if pid1 {
            power_off();
        }
    }));
    if pid1 {
        mount_filesystems();
    }
    report_machine();
    let status = match Cli::try_parse() {
        Ok(cli) => match cli.command {
            Mode::Run { args } => run(&args),
            Mode::Exchange { timeout } => {
                match Instant::now().checked_add(Duration::from_secs(timeout)) {
                    None => {
                        say(&format!(
                            "--timeout {timeout} lies past the latest time this clock can \
                             represent"
                        ));
                        2
                    }
                    Some(deadline) => match exchange(deadline) {
                        Ok(()) => 0,
                        Err(message) => {
                            say(&format!("exchange failed: {message}"));
                            1
                        }
                    },
                }
            }
        },
        Err(error) => {
            say(&error.to_string());
            2
        }
    };
    say(&format!("exit status {status}"));
    if pid1 {
        power_off();
    }
    std::process::exit(status);
}

fn say(message: &str) {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in message.lines() {
        let _ = writeln!(out, "spindle-init: {line}");
    }
    let _ = out.flush();
}

fn mount_filesystems() {
    for (source, target, kind) in [
        ("proc", "/proc", "proc"),
        ("sysfs", "/sys", "sysfs"),
        ("devtmpfs", "/dev", "devtmpfs"),
    ] {
        if let Err(error) = mount(source, target, kind) {
            say(&format!("mounting {kind} on {target}: {error}"));
        }
    }
}

fn mount(source: &str, target: &str, kind: &str) -> std::io::Result<()> {
    let source = std::ffi::CString::new(source)?;
    let target = std::ffi::CString::new(target)?;
    let kind = std::ffi::CString::new(kind)?;
    // SAFETY: every pointer is a NUL-terminated string that outlives the call; no data is passed.
    let result = unsafe {
        libc::mount(
            source.as_ptr(),
            target.as_ptr(),
            kind.as_ptr(),
            0,
            std::ptr::null(),
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

fn report_machine() {
    let cpus = std::thread::available_parallelism().map_or(0, std::num::NonZero::get);
    // SAFETY: `sysinfo` only writes into the zeroed struct it is given.
    let memory_mib = unsafe {
        let mut info: libc::sysinfo = std::mem::zeroed();
        if libc::sysinfo(&mut info) == 0 {
            info.totalram * u64::from(info.mem_unit) / (1024 * 1024)
        } else {
            0
        }
    };
    say(&format!("cpus={cpus} memory_mib={memory_mib}"));
    let mut variables: Vec<_> = std::env::vars()
        .filter(|(name, _)| name.starts_with("SPINDLE_"))
        .collect();
    variables.sort();
    for (name, value) in variables {
        say(&format!("{name}={value}"));
    }
}

fn power_off() -> ! {
    // SAFETY: `sync` and `reboot` take no pointers; PID 1 may reboot.
    unsafe {
        libc::sync();
        libc::reboot(libc::RB_AUTOBOOT);
    }
    loop {
        std::thread::park();
    }
}

fn run(args: &[String]) -> i32 {
    match Command::new("/bin/b10x-spindle")
        .args(args)
        .stdin(Stdio::null())
        .status()
    {
        Ok(status) => status.code().unwrap_or_else(|| {
            128 + std::os::unix::process::ExitStatusExt::signal(&status).unwrap_or(0)
        }),
        Err(error) => {
            say(&format!("starting /bin/b10x-spindle: {error}"));
            127
        }
    }
}

fn variable(name: &str) -> Result<String, String> {
    std::env::var(name).map_err(|_| format!("{name} is not set"))
}

fn exchange(deadline: Instant) -> Result<(), String> {
    let me = variable("SPINDLE_COMPONENT")?;
    let listen: SocketAddr = variable("SPINDLE_LISTEN")?
        .parse()
        .map_err(|error| format!("SPINDLE_LISTEN: {error}"))?;
    let mut peers = Vec::new();
    for (name, value) in std::env::vars() {
        if let Some(component) = name.strip_prefix("SPINDLE_PEER_") {
            for address in value.split(',') {
                let address: SocketAddr = address
                    .parse()
                    .map_err(|error| format!("{name}={value}: {error}"))?;
                peers.push((component.to_ascii_lowercase(), address));
            }
        }
    }
    peers.sort();
    if peers.is_empty() {
        return Err("no SPINDLE_PEER_<COMPONENT> is set".to_string());
    }

    let listener =
        TcpListener::bind(listen).map_err(|error| format!("binding {listen}: {error}"))?;
    let (served, greetings) = mpsc::channel();
    let name = me.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            if let Ok(greeting) = answer(stream, &name) {
                let _ = served.send(greeting);
            }
        }
    });

    for (peer, address) in &peers {
        let reply = greet(*address, &me, deadline)
            .map_err(|error| format!("{peer} at {address}: {error}"))?;
        say(&format!("{peer} at {address} answered: {reply}"));
    }
    let mut received = 0;
    while received < peers.len() {
        let left = deadline.saturating_duration_since(Instant::now());
        let greeting = greetings.recv_timeout(left).map_err(|_| {
            format!(
                "{received} of {} peers greeted this instance before the deadline",
                peers.len()
            )
        })?;
        say(&format!("received {greeting}"));
        received += 1;
    }
    Ok(())
}

/// Read one greeting line, answer it, and wait for the peer to close: the answer has arrived
/// before the greeting counts.
fn answer(stream: TcpStream, me: &str) -> std::io::Result<String> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut greeting = String::new();
    reader.read_line(&mut greeting)?;
    let greeting = greeting.trim().to_string();
    let mut writer = stream;
    writeln!(writer, "{me} got {greeting}")?;
    writer.flush()?;
    writer.shutdown(std::net::Shutdown::Write)?;
    let mut rest = String::new();
    while reader.read_line(&mut rest)? > 0 {}
    Ok(greeting)
}

fn greet(address: SocketAddr, me: &str, deadline: Instant) -> Result<String, String> {
    let mut last = String::from("no attempt");
    while Instant::now() < deadline {
        match TcpStream::connect_timeout(&address, Duration::from_secs(1)) {
            Ok(stream) => {
                let exchanged = (|| -> std::io::Result<String> {
                    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
                    let mut writer = stream.try_clone()?;
                    writeln!(writer, "hello from {me}")?;
                    writer.flush()?;
                    let mut reply = String::new();
                    BufReader::new(stream).read_line(&mut reply)?;
                    Ok(reply.trim().to_string())
                })();
                match exchanged {
                    Ok(reply) if !reply.is_empty() => return Ok(reply),
                    Ok(_) => last = "the connection closed without an answer".to_string(),
                    Err(error) => last = error.to_string(),
                }
            }
            Err(error) => last = error.to_string(),
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(format!("no answer before the deadline; last: {last}"))
}
