//! The pluggable clock, tested in a process of its own.
//!
//! `set_clock` is once-per-process on purpose, so this test cannot share a
//! binary with anything that wants real timestamps — integration tests each
//! get their own process, which is exactly the isolation it needs.

use hyperlab_stack::{Object, Stack, now_millis, set_clock};

#[test]
fn a_host_installed_clock_stamps_everything_and_cannot_be_replaced() {
    set_clock(|| 1_234_567);
    assert_eq!(now_millis(), 1_234_567);

    let stack = Stack::new("Notes");
    assert_eq!(stack.core().created_at, 1_234_567);

    // The second installer loses quietly: no host can swap the clock out
    // from under another mid-run.
    set_clock(|| 42);
    assert_eq!(now_millis(), 1_234_567);
}
