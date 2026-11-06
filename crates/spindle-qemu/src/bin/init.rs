//! `spindle-qemu-init`: PID 1 of a spindle microVM guest.
//!
//! Mounts `/dev`, `/proc` and `/sys`; loads the modules listed in `/etc/spindle/modules`, each
//! with the `<module>.<param>=` values of the kernel command line; brings up `lo` and gives `eth0`
//! the `spindle.addr=<ipv4>/<prefix>` address of the kernel command line; runs the command the
//! kernel passed after `--` as its child (with the variables of the launch plan's `environment`:
//! the instance, its component, listen address and peers, its settings and, when the launcher
//! gave one, the telemetry endpoint `SPINDLE_TELEMETRY`), relaying each line it
//! writes with the prefix `guest| `; prints `spindle-init: exit code=<n>` or
//! `spindle-init: exit signal=<n>` on the console; and powers the guest off. PID 1 never exits,
//! because the kernel panics when it does.
//!
//! The guest is Linux: built for another system, it refuses at once with status 2.

#[cfg(target_os = "linux")]
#[path = "init/guest.rs"]
mod guest;

#[cfg(target_os = "linux")]
fn main() {
    guest::main();
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("spindle-qemu-init runs as PID 1 of a Linux guest only");
    std::process::exit(2);
}
