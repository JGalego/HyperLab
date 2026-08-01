//! Moving old bundles forward.
//!
//! There is one format version so far, so there is nothing to migrate yet.
//! The seam exists now, before it is needed, because the alternative — adding
//! it once files already exist in the wild — is how formats become
//! unreadable.
//!
//! # The rules
//!
//! * An older bundle must always open. Add a step here that upgrades it.
//! * A newer bundle must never open silently. It is refused with a clear
//!   message rather than half-read.
//! * Unknown *properties* are not a version change: they round-trip
//!   untouched, so adding a property never needs a migration.

use crate::error::{PersistenceError, PersistenceResult};

/// Refuses a bundle from the future.
///
/// # Errors
///
/// Returns [`PersistenceError::UnsupportedVersion`] if `found` is newer than
/// this build understands.
pub fn check_version(found: u32, supported: u32) -> PersistenceResult<()> {
    if found > supported {
        return Err(PersistenceError::UnsupportedVersion { found, supported });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_and_current_bundles_are_accepted() {
        assert!(check_version(1, 1).is_ok());
        assert!(check_version(1, 5).is_ok());
    }

    #[test]
    fn a_newer_bundle_is_refused_rather_than_half_read() {
        let error = check_version(9, 1).unwrap_err();
        assert!(matches!(
            error,
            PersistenceError::UnsupportedVersion {
                found: 9,
                supported: 1
            }
        ));
    }
}
