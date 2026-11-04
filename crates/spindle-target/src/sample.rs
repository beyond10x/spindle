//! Host-side resource samples: what a running instance costs the host, read from `/proc`, and its
//! share of the resources it asked for (`docs/design/observe.md` § 4).
//!
//! CPU seconds are user and system time (`utime` + `stime`, and the same of children the process
//! has waited for) from `/proc/<pid>/stat`; memory is the resident set (`VmRSS`) from
//! `/proc/<pid>/status`. A process group is the sum over every process whose group id is the
//! group's. Shared pages are counted once per process that maps them.

use std::fs;
use std::io;
use std::time::Duration;

use crate::Resources;

const MIB: f64 = (1u64 << 20) as f64;

/// What one instance costs the host since launch, and its share of the quota it asked for.
///
/// `cpu_share = cpu_seconds / (wall_seconds × requested vCPUs)` and
/// `memory_share = memory_bytes / (requested memory_mib × 2^20)`. A share is `None` when that
/// resource was not asked for (a zero request has no quota) or, for CPU, when no wall time has
/// passed yet.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct ResourceSample {
    /// CPU seconds the instance has used since launch, on every CPU together.
    pub cpu_seconds: f64,
    /// Resident memory now, in bytes.
    pub memory_bytes: u64,
    /// Wall seconds since launch.
    pub wall_seconds: f64,
    /// `cpu_seconds` over the CPU time the requested vCPUs offered since launch.
    pub cpu_share: Option<f64>,
    /// `memory_bytes` over the requested memory.
    pub memory_share: Option<f64>,
}

impl ResourceSample {
    /// A sample of `cpu_seconds` and `memory_bytes` taken `wall_seconds` after launch, measured
    /// against `request`.
    pub fn against(
        cpu_seconds: f64,
        memory_bytes: u64,
        wall_seconds: f64,
        request: &Resources,
    ) -> Self {
        let cpu_share = (request.vcpus != 0 && wall_seconds > 0.0)
            .then(|| cpu_seconds / (wall_seconds * f64::from(request.vcpus)));
        let memory_share = (request.memory_mib != 0)
            .then(|| memory_bytes as f64 / (request.memory_mib as f64 * MIB));
        Self {
            cpu_seconds,
            memory_bytes,
            wall_seconds,
            cpu_share,
            memory_share,
        }
    }

    /// The sample of `usage`, `since` launch, against `request`.
    pub fn of(usage: HostUsage, since: Duration, request: &Resources) -> Self {
        Self::against(
            usage.cpu_seconds,
            usage.memory_bytes,
            since.as_secs_f64(),
            request,
        )
    }
}

/// CPU time and resident memory of one process or a process group, as the host sees them.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize)]
pub struct HostUsage {
    /// User and system CPU seconds.
    pub cpu_seconds: f64,
    /// Resident memory, in bytes.
    pub memory_bytes: u64,
}

struct Stat {
    state: char,
    group: u32,
    ticks: u64,
}

/// The usage of process `pid` (all its threads, and the children it has waited for). A process
/// that has ended but is not yet reaped is `NotFound`.
pub fn process_usage(pid: u32) -> io::Result<HostUsage> {
    let stat = stat(pid)?;
    if stat.state == 'Z' {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("process {pid} has ended"),
        ));
    }
    Ok(HostUsage {
        cpu_seconds: seconds(stat.ticks),
        memory_bytes: resident_bytes(pid)?,
    })
}

/// The usage of process group `group`: the sum over every process in it. A process that ends
/// while the group is read is left out; a group with no process is `NotFound`.
pub fn process_group_usage(group: u32) -> io::Result<HostUsage> {
    let mut ticks = 0u64;
    let mut memory_bytes = 0u64;
    let mut members = 0usize;
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let member = stat(pid).and_then(|stat| {
            if stat.group == group {
                Ok(Some((stat.ticks, resident_bytes(pid)?)))
            } else {
                Ok(None)
            }
        });
        match member {
            Ok(Some((cpu, resident))) => {
                ticks = ticks.saturating_add(cpu);
                memory_bytes = memory_bytes.saturating_add(resident);
                members += 1;
            }
            Ok(None) => {}
            Err(error) if gone(&error) => {}
            Err(error) => return Err(error),
        }
    }
    if members == 0 {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no process is in group {group}"),
        ));
    }
    Ok(HostUsage {
        cpu_seconds: seconds(ticks),
        memory_bytes,
    })
}

fn gone(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::NotFound
        || error.raw_os_error() == Some(rustix::io::Errno::SRCH.raw_os_error())
}

fn seconds(ticks: u64) -> f64 {
    ticks as f64 / rustix::param::clock_ticks_per_second() as f64
}

/// The fields of `/proc/<pid>/stat` after the command name, which may itself hold spaces and
/// parentheses: the state (field 3), the group id (field 5) and `utime + stime + cutime + cstime` (fields 14 to 17).
fn stat(pid: u32) -> io::Result<Stat> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let malformed = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("/proc/{pid}/stat is not in the expected format"),
        )
    };
    let (_, rest) = text.rsplit_once(')').ok_or_else(malformed)?;
    let fields: Vec<&str> = rest.split_ascii_whitespace().collect();
    let field = |number: usize| -> io::Result<u64> {
        fields
            .get(number - 3)
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(malformed)
    };
    let state = fields
        .first()
        .and_then(|state| state.chars().next())
        .ok_or_else(malformed)?;
    let group = u32::try_from(field(5)?).map_err(|_| malformed())?;
    let ticks = (14..=17).try_fold(0u64, |sum, number| {
        Ok::<_, io::Error>(sum.saturating_add(field(number)?))
    })?;
    Ok(Stat {
        state,
        group,
        ticks,
    })
}

/// `VmRSS` of `/proc/<pid>/status` in bytes; zero for a process without one (a zombie).
fn resident_bytes(pid: u32) -> io::Result<u64> {
    let text = fs::read_to_string(format!("/proc/{pid}/status"))?;
    Ok(text
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|rest| rest.trim().strip_suffix("kB"))
        .and_then(|kib| kib.trim().parse::<u64>().ok())
        .map_or(0, |kib| kib.saturating_mul(1024)))
}
