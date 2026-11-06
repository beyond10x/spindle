//! What an instance reads of its own resources, inside the instance.

use std::fs;
use std::io;

use crate::frame::ResourceReading;

/// Whose resources an instance reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Readings {
    /// The process's own: `/proc/self/stat` (user and system time) and `/proc/self/status`
    /// (resident memory). A process instance shares its machine, so the machine's figures are
    /// not its own.
    Process,
    /// The whole machine's: `/proc/stat` (every CPU's time other than idle and I/O wait) and
    /// `/proc/meminfo` (total less available). A microVM guest is the instance.
    Machine,
}

/// What [`read_resources`] answers off Linux, where there is no `/proc` to read.
pub const READINGS_REFUSAL: &str = "resource readings come from /proc, which only Linux has";

/// Reads `readings` now; refused as [`io::ErrorKind::Unsupported`] off Linux
/// ([`READINGS_REFUSAL`]), where a sender sends zero readings.
pub fn read_resources(readings: Readings) -> io::Result<ResourceReading> {
    if !cfg!(target_os = "linux") {
        return Err(io::Error::new(io::ErrorKind::Unsupported, READINGS_REFUSAL));
    }
    let ticks = clock_ticks();
    let (ticks_used, memory_bytes) = match readings {
        Readings::Process => (
            process_ticks(&fs::read_to_string("/proc/self/stat")?)?,
            status_bytes(&fs::read_to_string("/proc/self/status")?, "VmRSS:")?,
        ),
        Readings::Machine => (
            machine_ticks(&fs::read_to_string("/proc/stat")?)?,
            meminfo_in_use(&fs::read_to_string("/proc/meminfo")?)?,
        ),
    };
    Ok(ResourceReading {
        cpu_seconds: ticks_used as f64 / ticks as f64,
        memory_bytes,
        cpu_share: None,
        memory_share: None,
    })
}

fn invalid(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("unreadable {what}"))
}

fn clock_ticks() -> u64 {
    // SAFETY: sysconf takes no pointers.
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    u64::try_from(ticks)
        .ok()
        .filter(|ticks| *ticks > 0)
        .unwrap_or(100)
}

/// utime + stime, fields 14 and 15 of `/proc/self/stat`, counted after the `)` that ends the
/// command name, which may itself hold spaces.
fn process_ticks(stat: &str) -> io::Result<u64> {
    let rest = stat
        .rsplit_once(')')
        .map(|(_, rest)| rest)
        .ok_or_else(|| invalid("/proc/self/stat"))?;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    let field = |index: usize| -> io::Result<u64> {
        fields
            .get(index)
            .and_then(|value| value.parse().ok())
            .ok_or_else(|| invalid("/proc/self/stat"))
    };
    Ok(field(11)? + field(12)?)
}

/// The `cpu` line of `/proc/stat` less idle (4th), iowait (5th) and steal (8th): steal is time
/// the hypervisor ran something else, not the guest. Guest time is already in user.
fn machine_ticks(stat: &str) -> io::Result<u64> {
    let line = stat
        .lines()
        .find(|line| line.starts_with("cpu "))
        .ok_or_else(|| invalid("/proc/stat"))?;
    let values: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .take(8)
        .map(|value| value.parse().map_err(|_| invalid("/proc/stat")))
        .collect::<io::Result<_>>()?;
    Ok(values
        .iter()
        .enumerate()
        .filter(|(index, _)| !matches!(index, 3 | 4 | 7))
        .map(|(_, value)| value)
        .sum())
}

fn kib(text: &str, key: &str, what: &str) -> io::Result<u64> {
    text.lines()
        .find_map(|line| line.strip_prefix(key))
        .and_then(|rest| {
            rest.trim()
                .trim_end_matches("kB")
                .trim()
                .parse::<u64>()
                .ok()
        })
        .map(|kib| kib * 1024)
        .ok_or_else(|| invalid(what))
}

fn status_bytes(status: &str, key: &str) -> io::Result<u64> {
    kib(status, key, "/proc/self/status")
}

fn meminfo_in_use(meminfo: &str) -> io::Result<u64> {
    let total = kib(meminfo, "MemTotal:", "/proc/meminfo")?;
    let available = kib(meminfo, "MemAvailable:", "/proc/meminfo")
        .or_else(|_| kib(meminfo, "MemFree:", "/proc/meminfo"))?;
    Ok(total.saturating_sub(available))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_texts_read_as_ticks_and_bytes() {
        let stat = "4242 (a name) with) S 1 2 3 4 5 6 7 8 9 10 31 9 0 0";
        assert_eq!(process_ticks(stat).expect("readable"), 40);
        let machine = "cpu  10 1 5 900 7 2 3 4 0 0\ncpu0 10 1 5 900 7 2 3 4 0 0\n";
        assert_eq!(machine_ticks(machine).expect("readable"), 21);
        let meminfo = "MemTotal:       2000 kB\nMemFree:  100 kB\nMemAvailable:   500 kB\n";
        assert_eq!(meminfo_in_use(meminfo).expect("readable"), 1500 * 1024);
        assert_eq!(
            status_bytes("Name: x\nVmRSS:\t  12 kB\n", "VmRSS:").expect("readable"),
            12 * 1024
        );
    }
}
