//! The rally example on three QEMU microVMs, one `b10x-spindle play` per VM in its per-component
//! mode, joined by TCP channels across the VM network (design § 12, M3).
//!
//! `#[ignore]`d: run by `task check:vm`. Where `/dev/kvm`, `qemu-system-x86_64`, the host kernel
//! with its virtio modules or the `x86_64-unknown-linux-musl` target is absent, the case prints
//! why and returns.

#[path = "support/rally.rs"]
mod rally;

#[test]
#[ignore = "boots three QEMU microVMs: needs /dev/kvm, qemu-system-x86_64, the host kernel and its virtio modules and the x86_64-unknown-linux-musl target"]
fn a_match_between_three_qemu_microvms_ends_with_a_winner() {
    let qemu = match rally::qemu() {
        Ok(qemu) => qemu,
        Err(reason) => {
            return rally::skip(
                "a_match_between_three_qemu_microvms_ends_with_a_winner",
                &reason,
            );
        }
    };
    let placement = rally::placement();
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a work directory");
    let artifact = qemu.artifact(dir.path(), &placement.play_arguments());
    let mut launcher = qemu.launcher(dir.path());

    let played = rally::play(&mut launcher, &placement.plans(&artifact));

    assert!(
        ["West", "East"].contains(&played.winner()),
        "{:?}",
        played.view
    );
}
