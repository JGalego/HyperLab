//! HyperTalk, translated into [_hyperscript](https://hyperscript.org).
//!
//! _hyperscript is HyperTalk's descendant. Carson Gross wrote it for the web
//! after the same language HyperLab reimplements, and it shows: `put x into
//! y`, `set x to y`, `if … then … end`, `repeat while`, `is not`, `starts
//! with` and `contains` all mean in one what they mean in the other. Most of
//! this crate is therefore not a compiler so much as a change of address.
//!
//! [`page`] turns a whole stack into one HTML file that runs in a browser
//! with no HyperLab in sight. [`script`] does only the language part, for
//! looking at what a handler becomes.
//!
//! ```
//! let out = hyperlab_hyperscript::script("on mouseUp\n  put \"hi\" into it\nend mouseUp")?;
//! assert!(out.source.contains("on click"));
//! # Ok::<(), String>(())
//! ```
//!
//! # Where the two part company
//!
//! Every difference below was found by running the real library in a browser
//! rather than by reading about it, and each one is a place a careless
//! translation would emit something that parses and does nothing.
//!
//! * **`it` cannot be assigned.** In _hyperscript `it` is the previous
//!   command's result. `set it to …` is accepted and quietly yields `null`, so
//!   HyperTalk's `it` becomes an ordinary variable named `hlIt`.
//! * **There is no `repeat with i = 1 to n`.** It is a parse error. The
//!   translation counts with `repeat … times index` and derives the loop
//!   variable, which keeps `next repeat` honest — a hand-rolled counter would
//!   be skipped by `continue` and loop for ever.
//! * **A field is an element, not a container.** `put x into #f` sets the
//!   markup inside a div and the *value* of a textarea, so fields are written
//!   through `'s value`.
//!
//! What has no equivalent at all is not guessed at. It becomes a comment in
//! the output and a line in [`Translation::notes`], so the gap is visible in
//! the file and countable by the caller.

#![warn(missing_docs)]

mod html;
mod page;
mod script;

pub use page::page;
pub use script::script;

/// Translated source, and what could not be carried across.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Translation {
    /// The _hyperscript, or the whole HTML page.
    pub source: String,
    /// One line for everything that did not translate, in the order met.
    ///
    /// Empty means the whole thing came across. It is never a silent partial
    /// success: whatever is listed here is also a comment where it belonged.
    pub notes: Vec<String>,
}

impl Translation {
    /// Whether everything translated.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.notes.is_empty()
    }
}
