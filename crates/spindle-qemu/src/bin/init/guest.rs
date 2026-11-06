//! The guest side of `spindle-qemu-init`, built for Linux only.

use std::ffi::CString;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::Ipv4Addr;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const MODULE_LIST: &str = "/etc/spindle/modules";
const NO_COMMAND: i32 = 127;
use spindle_qemu::{EXIT_REPORT, GUEST_OUTPUT};

const RELAY_DRAIN: Duration = Duration::from_secs(2);

fn say(message: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "spindle-init: {message}");
    let _ = out.flush();
}

pub(super) fn main() {
    if std::process::id() != 1 {
        eprintln!("spindle-qemu-init runs as PID 1 of a guest only");
        std::process::exit(2);
    }
    for (source, target, kind) in [
        ("devtmpfs", "/dev", "devtmpfs"),
        ("proc", "/proc", "proc"),
        ("sysfs", "/sys", "sysfs"),
        ("tmpfs", "/tmp", "tmpfs"),
    ] {
        if let Err(error) = mount(source, target, kind) {
            say(&format!("mount {target}: {error}"));
        }
    }
    let cmdline = fs::read_to_string("/proc/cmdline").unwrap_or_default();
    load_modules(&cmdline);
    if let Err(error) = configure("lo", Ipv4Addr::LOCALHOST, 8) {
        say(&format!("lo: {error}"));
    }
    if let Some((address, prefix)) = parameter(&cmdline, "spindle.addr").and_then(parse_address) {
        match wait_for_interface("eth0", Duration::from_secs(10))
            .and_then(|()| configure("eth0", address, prefix))
        {
            Ok(()) => say(&format!("eth0 {address}/{prefix}")),
            Err(error) => say(&format!("eth0: {error}")),
        }
    }
    let report = run(std::env::args().skip(1).collect());
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{EXIT_REPORT}{report}");
    let _ = out.flush();
    drop(out);
    power_off();
}

fn run(argv: Vec<String>) -> String {
    let Some((program, args)) = argv.split_first() else {
        say("no command after -- on the kernel command line");
        return format!("code={NO_COMMAND}");
    };
    let child = Command::new(program)
        .args(args)
        .env_clear()
        .env("PATH", "/bin")
        .env("HOME", "/")
        .envs(launch_variables())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let status = child.and_then(|mut child| {
        let (done, finished) = mpsc::channel();
        let mut relays = 0;
        if let Some(out) = child.stdout.take() {
            relays += 1;
            relay(out, done.clone());
        }
        if let Some(err) = child.stderr.take() {
            relays += 1;
            relay(err, done);
        }
        let status = child.wait();
        for _ in 0..relays {
            if finished.recv_timeout(RELAY_DRAIN).is_err() {
                say("the command's output did not close; its report follows anyway");
                break;
            }
        }
        status
    });
    match status {
        Ok(status) => match (status.code(), status.signal()) {
            (Some(code), _) => format!("code={code}"),
            (None, Some(signal)) => format!("signal={signal}"),
            (None, None) => format!("code={NO_COMMAND}"),
        },
        Err(error) => {
            say(&format!("cannot run {program}: {error}"));
            format!("code={NO_COMMAND}")
        }
    }
}

/// The variables the guest command gets from the launch plan: every entry of its `environment`
/// (`SPINDLE_INSTANCE_ID`, `SPINDLE_COMPONENT`, `SPINDLE_LISTEN`, `SPINDLE_PEER_<COMPONENT>`,
/// `SPINDLE_SETTING_<NAME>`, and `SPINDLE_TELEMETRY` when the launcher gave the guest a telemetry
/// endpoint).
fn launch_variables() -> Vec<(String, String)> {
    let Ok(text) = fs::read_to_string(spindle_qemu::LAUNCH_PLAN) else {
        return Vec::new();
    };
    let Ok(plan) = serde_json::from_str::<serde_json::Value>(&text) else {
        say("the launch plan is not JSON");
        return Vec::new();
    };
    let Some(environment) = plan["environment"].as_object() else {
        say("the launch plan has no environment");
        return Vec::new();
    };
    environment
        .iter()
        .filter_map(|(name, value)| Some((name.clone(), value.as_str()?.to_owned())))
        .collect()
}

/// Copy one output stream of the guest command to the console, each line prefixed with
/// [`GUEST_OUTPUT`], so the command cannot write a line that looks like this init's own; a last
/// line without a newline gets one. Signals `done` at end of stream.
fn relay(stream: impl Read + Send + 'static, done: mpsc::Sender<()>) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut line = Vec::new();
        loop {
            line.clear();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if line.last() != Some(&b'\n') {
                        line.push(b'\n');
                    }
                    let mut out = std::io::stdout().lock();
                    let _ = out.write_all(GUEST_OUTPUT.as_bytes());
                    let _ = out.write_all(&line);
                    let _ = out.flush();
                }
            }
        }
        let _ = done.send(());
    });
}

fn last_os_error(what: &str) -> String {
    format!("{what}: {}", std::io::Error::last_os_error())
}

fn mount(source: &str, target: &str, kind: &str) -> Result<(), String> {
    let _ = fs::create_dir_all(target);
    let source = CString::new(source).map_err(|error| error.to_string())?;
    let target_c = CString::new(target).map_err(|error| error.to_string())?;
    let kind = CString::new(kind).map_err(|error| error.to_string())?;
    // SAFETY: every pointer is a NUL-terminated string that outlives the call; data is null.
    let result = unsafe {
        libc::mount(
            source.as_ptr(),
            target_c.as_ptr(),
            kind.as_ptr(),
            0,
            std::ptr::null(),
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(last_os_error("mount"))
    }
}

/// The value of `name=` on the kernel command line.
fn parameter<'a>(cmdline: &'a str, name: &str) -> Option<&'a str> {
    cmdline
        .split_whitespace()
        .take_while(|word| *word != "--")
        .find_map(|word| word.strip_prefix(name)?.strip_prefix('='))
}

fn parse_address(value: &str) -> Option<(Ipv4Addr, u8)> {
    let (address, prefix) = value.split_once('/')?;
    let prefix: u8 = prefix.parse().ok()?;
    (prefix <= 32).then_some((address.parse().ok()?, prefix))
}

fn load_modules(cmdline: &str) {
    let Ok(list) = fs::read_to_string(MODULE_LIST) else {
        return;
    };
    for line in list.lines() {
        let Some((name, path)) = line.split_once(' ') else {
            continue;
        };
        let prefix = format!("{name}.");
        let params: Vec<&str> = cmdline
            .split_whitespace()
            .take_while(|word| *word != "--")
            .filter_map(|word| word.strip_prefix(prefix.as_str()))
            .collect();
        if let Err(error) = load_module(Path::new(path), &params.join(" ")) {
            say(&format!("module {name}: {error}"));
        }
    }
}

fn load_module(path: &Path, params: &str) -> Result<(), String> {
    let image = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let params = CString::new(params).map_err(|error| error.to_string())?;
    // SAFETY: init_module reads `image.len()` bytes from `image` and a NUL-terminated string.
    let result = unsafe {
        libc::syscall(
            libc::SYS_init_module,
            image.as_ptr(),
            image.len(),
            params.as_ptr(),
        )
    };
    if result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EEXIST) {
        Ok(())
    } else {
        Err(last_os_error("init_module"))
    }
}

fn wait_for_interface(name: &str, within: Duration) -> Result<(), String> {
    let path = format!("/sys/class/net/{name}");
    let deadline = Instant::now() + within;
    while !Path::new(&path).exists() {
        if Instant::now() >= deadline {
            return Err(format!("no interface {name} after {within:?}"));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

fn request(name: &str) -> Result<libc::ifreq, String> {
    let bytes = name.as_bytes();
    if bytes.len() >= libc::IFNAMSIZ {
        return Err(format!("interface name {name} is too long"));
    }
    // SAFETY: ifreq is plain old data; all-zero is a valid value.
    let mut ifr: libc::ifreq = unsafe { std::mem::zeroed() };
    for (slot, byte) in ifr.ifr_name.iter_mut().zip(bytes) {
        *slot = *byte as libc::c_char;
    }
    Ok(ifr)
}

fn sockaddr(address: Ipv4Addr) -> libc::sockaddr {
    let inet = libc::sockaddr_in {
        sin_family: libc::AF_INET as libc::sa_family_t,
        sin_port: 0,
        sin_addr: libc::in_addr {
            s_addr: u32::from_ne_bytes(address.octets()),
        },
        sin_zero: [0; 8],
    };
    // SAFETY: sockaddr_in and sockaddr have the same size on Linux; the kernel reads the
    // AF_INET layout back out of it.
    unsafe { std::mem::transmute::<libc::sockaddr_in, libc::sockaddr>(inet) }
}

struct Socket(libc::c_int);

impl Drop for Socket {
    fn drop(&mut self) {
        // SAFETY: the descriptor is owned by this value.
        unsafe { libc::close(self.0) };
    }
}

fn ioctl(
    socket: &Socket,
    request: libc::c_ulong,
    ifr: &mut libc::ifreq,
    what: &str,
) -> Result<(), String> {
    // SAFETY: `ifr` is a valid ifreq for the duration of the call.
    let result = unsafe { libc::ioctl(socket.0, request as _, ifr as *mut libc::ifreq) };
    if result == 0 {
        Ok(())
    } else {
        Err(last_os_error(what))
    }
}

fn configure(name: &str, address: Ipv4Addr, prefix: u8) -> Result<(), String> {
    // SAFETY: a plain socket(2) call.
    let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0) };
    if fd < 0 {
        return Err(last_os_error("socket"));
    }
    let socket = Socket(fd);
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - u32::from(prefix))
    };

    let mut ifr = request(name)?;
    ifr.ifr_ifru.ifru_addr = sockaddr(address);
    ioctl(
        &socket,
        libc::SIOCSIFADDR as libc::c_ulong,
        &mut ifr,
        "SIOCSIFADDR",
    )?;

    let mut ifr = request(name)?;
    ifr.ifr_ifru.ifru_netmask = sockaddr(Ipv4Addr::from(mask));
    ioctl(
        &socket,
        libc::SIOCSIFNETMASK as libc::c_ulong,
        &mut ifr,
        "SIOCSIFNETMASK",
    )?;

    let mut ifr = request(name)?;
    ioctl(
        &socket,
        libc::SIOCGIFFLAGS as libc::c_ulong,
        &mut ifr,
        "SIOCGIFFLAGS",
    )?;
    // SAFETY: SIOCGIFFLAGS filled the flags member of the union.
    let flags = unsafe { ifr.ifr_ifru.ifru_flags };
    ifr.ifr_ifru.ifru_flags = flags | (libc::IFF_UP | libc::IFF_RUNNING) as libc::c_short;
    ioctl(
        &socket,
        libc::SIOCSIFFLAGS as libc::c_ulong,
        &mut ifr,
        "SIOCSIFFLAGS",
    )
}

fn power_off() -> ! {
    // SAFETY: sync and reboot take no pointers; PID 1 is allowed to reboot.
    unsafe {
        libc::sync();
        libc::reboot(libc::RB_POWER_OFF);
        libc::reboot(libc::RB_AUTOBOOT);
    }
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}
