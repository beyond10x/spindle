//! `/init` of a Spindle guest: a static binary the kernel starts as PID 1.
//!
//! It mounts `/proc`, `/sys` and `/dev`, reports the vCPUs, memory and `SPINDLE_*` variables it
//! sees, runs its command, prints `spindle-init: exit status <n>` and reboots, which ends the
//! Firecracker process (`reboot=k`). The network is already up: the kernel configures `eth0`
//! from `ip=` on its command line. Run as any other PID it exits with the status instead.
//!
//! The guest is Linux: built for another system, it refuses at once with status 2.

#[cfg(target_os = "linux")]
#[path = "spindle-guest-init/guest.rs"]
mod guest;

#[cfg(target_os = "linux")]
fn main() {
    guest::main();
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("spindle-guest-init runs as PID 1 of a Linux guest only");
    std::process::exit(2);
}
