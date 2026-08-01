//! The one place in HyperLab that holds an API key.
//!
//! Keys go into whatever secret store the operating system already runs —
//! Keychain Services, the Credential Manager, the Secret Service — filed
//! under the name the user gave the provider. Nothing is written to disk by
//! HyperLab, and [`ai.json`](crate::settings) records only the word
//! `keychain`, so it can still be copied into a bug report.
//!
//! Three rules, and the tests below are about all three.
//!
//! **A key comes in and never goes back out.** [`set`] takes one; there is no
//! function here that returns one to the interface, and
//! [`SystemKeychain::key`] hands it only to the provider that is about to
//! send it. The panel can ask *whether* a key is saved ([`holds`]) and never
//! what it is.
//!
//! **A missing keychain is a sentence, not a crash.** Linux only has a Secret
//! Service if something is running one, and a machine without one should
//! still be able to use an environment variable. [`available`] says which
//! situation this is, so the panel can offer what will actually work.
//!
//! **A key is never in an error message.** Everything below reports the
//! store's complaint, and the store is never told the key when it fails.

use hyperlab_ai::Keychain;

/// What HyperLab's entries are filed under.
///
/// One service for the application, with the provider's name as the account,
/// which is how a person browsing their keychain would expect to find them.
const SERVICE: &str = "HyperLab";

/// The operating system's keychain, as the AI layer sees it.
pub struct SystemKeychain;

impl Keychain for SystemKeychain {
    fn key(&self, provider: &str) -> Option<String> {
        keyring::Entry::new(SERVICE, provider)
            .ok()?
            .get_password()
            .ok()
    }
}

/// Whether this machine has a keychain HyperLab can reach.
///
/// Reaching one is not the same as being able to write to it — a Secret
/// Service with every collection locked answers here and refuses [`set`] —
/// so this decides whether the panel *offers* the keychain, and [`set`]
/// decides whether a particular key goes in.
///
/// # Errors
///
/// Returns a sentence to show. On Linux this is usually that nothing is
/// running a Secret Service, which is worth saying plainly: the fix is to
/// install one or to use an environment variable, and neither is guessable
/// from "could not save".
pub fn available() -> Result<(), String> {
    match keyring::Entry::store_status() {
        Ok(()) => Ok(()),
        Err(error) => Err(format!(
            "this machine has no keychain HyperLab can use: {error}"
        )),
    }
}

/// Saves a provider's key.
///
/// # Errors
///
/// Returns a sentence to show if there is no keychain or it refuses.
pub fn set(provider: &str, key: &str) -> Result<(), String> {
    if provider.is_empty() {
        return Err("a key has to belong to a provider with a name".into());
    }
    if key.is_empty() {
        return Err("there is no key to save".into());
    }
    entry(provider)?
        .set_password(key)
        .map_err(|error| format!("could not save the key: {error}"))
}

/// Forgets a provider's key.
///
/// A provider with no key saved is already forgotten, so that is a success
/// rather than an error — the button does what it says either way.
///
/// # Errors
///
/// Returns a sentence to show if there is no keychain or it refuses.
pub fn forget(provider: &str) -> Result<(), String> {
    match entry(provider)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(format!("could not forget the key: {error}")),
    }
}

/// Whether a key is saved for this provider.
///
/// Deliberately a `bool`. The panel needs to know that there is one, and has
/// no business knowing what it is.
#[must_use]
pub fn holds(provider: &str) -> bool {
    SystemKeychain.key(provider).is_some()
}

/// This provider's entry, or why there is not one.
fn entry(provider: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, provider)
        .map_err(|error| format!("this machine has no keychain HyperLab can use: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_here_can_be_asked_for_a_key_by_name() {
        // `holds` answers yes or no, and the only thing that returns a key is
        // the `Keychain` implementation the provider layer calls. This test
        // is a tripwire: it fails to compile if `holds` ever starts handing
        // the value back to whoever asks.
        let answer: bool = holds("a provider that does not exist");
        assert!(!answer);
    }

    #[test]
    fn saving_nothing_is_refused_before_the_keychain_is_touched() {
        // Both of these fail on the arguments, so they hold on a machine with
        // no keychain at all — which is most build machines.
        assert!(set("", "a-key").is_err());
        assert!(set("work", "").is_err());
    }

    #[test]
    fn a_machine_with_no_keychain_says_so_in_a_sentence() {
        // Whichever way this goes, it is a sentence and not a panic. CI has no
        // Secret Service; a developer's laptop does.
        if let Err(reason) = available() {
            assert!(
                reason.starts_with("this machine has no keychain"),
                "got {reason}"
            );
        }
    }

    #[test]
    fn a_key_survives_being_saved_and_can_be_forgotten() {
        let provider = format!("hyperlab-test-{}", std::process::id());

        // Three machines run this: one with no keychain, one with a keychain
        // that is locked, and one with a keychain that works. Only the last
        // can round-trip, and the other two have to fail in a sentence rather
        // than fail the build — a locked Secret Service answers `available`
        // and still refuses to be written to.
        let Ok(()) = set(&provider, "a-key") else {
            let refusal = set(&provider, "a-key").unwrap_err();
            assert!(
                refusal.starts_with("could not save the key")
                    || refusal.starts_with("this machine has no keychain"),
                "a refusal has to be readable: {refusal}"
            );
            return;
        };

        assert!(holds(&provider));
        assert_eq!(SystemKeychain.key(&provider).as_deref(), Some("a-key"));

        forget(&provider).expect("saved, so removable");
        assert!(!holds(&provider));
        // And again, because a button that reports failure the second time is
        // a button people press twice.
        forget(&provider).expect("forgetting nothing is not a failure");
    }
}
