//! HyperLab in a browser.
//!
//! The desktop application is a Tauri shell around the same crates this one
//! wraps; here the shell is a WebAssembly module and the window is a web
//! page. The command surface is deliberately the same, function for
//! function, so the React renderer cannot tell which shell it is behind.
//!
//! What is different is everything a browser does differently:
//!
//! * **Dialogs.** The desktop blocks a script on a channel while the window
//!   shows the dialog. Here the runtime runs in a Web Worker and the
//!   [`Host`](hyperlab_runtime::Host) callbacks cross into JavaScript, which
//!   blocks the worker on `Atomics.wait` while the page shows the same
//!   dialog. Neither side of that trade lives in this crate: the host is a
//!   JavaScript object handed to [`api::init`].
//! * **Files.** There is no file system. A stack travels as the single-file
//!   JSON `hyperlab-persistence` already speaks, uploaded and downloaded by
//!   the page.
//! * **Keys.** There is no OS keychain. A provider's key goes into browser
//!   storage through the host object, is sent only to the provider the user
//!   configured, and there is no call that reads one back out — the same
//!   one-way rule the desktop keeps.
//! * **AI transport.** The wire protocol comes from `hyperlab-ai-providers`
//!   (built without its native clients); the bytes travel through `fetch`,
//!   so a request goes straight from the user's browser to their provider
//!   and never through the server that served the page.

#![warn(missing_docs)]

#[cfg(target_arch = "wasm32")]
pub mod api;
pub mod providers;
pub mod view;
