//! Guest context ids: one count per process. This binary holds one case, since it takes them all.

use std::collections::BTreeSet;

use spindle_qemu::{LaunchError, MAX_GUESTS_PER_PROCESS, next_cid};

#[test]
fn a_process_gives_out_1024_distinct_context_ids_and_refuses_the_next() {
    let mut seen = BTreeSet::new();
    for _ in 0..MAX_GUESTS_PER_PROCESS {
        let cid = next_cid().expect("a context id");
        assert!(
            (3..u32::MAX).contains(&cid),
            "{cid} is not a guest context id"
        );
        assert!(seen.insert(cid), "{cid} was given out twice");
    }
    match next_cid() {
        Err(LaunchError::InvalidPlan(message)) => assert!(
            message.contains("vsock guest context ids"),
            "the refusal names what ran out: {message}"
        ),
        other => panic!("the 1025th context id is not refused: {other:?}"),
    }
}
