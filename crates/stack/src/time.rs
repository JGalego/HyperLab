//! Wall-clock timestamps.
//!
//! Objects record when they were created and last changed. The clock is
//! isolated here so a host can replace it — which stopped being theoretical
//! the day HyperLab compiled to WebAssembly: `SystemTime::now` traps in a
//! browser, so the web shell names the browser's own clock instead.

use std::sync::OnceLock;

/// The clock a host installed, when one did.
static CLOCK: OnceLock<fn() -> u64> = OnceLock::new();

/// Names the clock, for hosts on platforms where the standard one traps —
/// WebAssembly in a browser. `clock` answers in milliseconds since the Unix
/// epoch.
///
/// The first call wins and later calls change nothing, so no host can swap
/// the clock out from under another mid-run.
pub fn set_clock(clock: fn() -> u64) {
    let _ = CLOCK.set(clock);
}

/// Milliseconds since the Unix epoch.
#[must_use]
pub fn now_millis() -> u64 {
    match CLOCK.get() {
        Some(clock) => clock(),
        None => platform_now(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn platform_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// The epoch, on a platform with no clock of its own. A host that cares —
/// the web shell does — installs one with [`set_clock`] before anything is
/// stamped.
#[cfg(target_arch = "wasm32")]
fn platform_now() -> u64 {
    0
}
