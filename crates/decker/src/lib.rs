//! A stack as a [Decker](https://beyondloom.com/decker/) deck.
//!
//! Decker is John Earnest's multimedia platform, and another descendant of
//! HyperCard: cards, widgets, one-bit artwork, and a scripting language of its
//! own called Lil. [`deck`] writes a stack as a `.deck` file that opens in it.
//!
//! ```no_run
//! let stack = hyperlab_persistence::load("examples/Cluedo.hl")?;
//! let written = hyperlab_decker::deck(&stack);
//! std::fs::write("Cluedo.deck", &written.source)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # What crosses, and what does not
//!
//! Buttons and fields become widgets where they sit, with their text, and
//! pictures are drawn into the card's background bitmap — Decker keeps
//! artwork behind the widgets rather than as one of them. A picture with a
//! script keeps an invisible button over it, so it can still be clicked.
//!
//! Lil is not a HyperTalk. Assignment is `x:1`, a call is `f[a b]`, `if` takes
//! no `then`, it evaluates right to left, and there is no `break`, no early
//! `return` and no chunks. So navigation, field assignment, `alert`,
//! conditionals, arithmetic, loops and text sliced by line, word, item or
//! character all translate; what does not becomes a `#` comment where it
//! belonged and a line in [`Translation::notes`].
//!
//! Two things a deck has no room for at all: a handler that asks a language
//! model, and the moment a stack opens — a deck starts on its first card, and
//! its own `view` is every card's arrival rather than the deck's.
//!
//! Everything about the file format was settled by writing decks by hand and
//! opening them in Decker, which is how the awkward parts were found: a
//! field's contents are `value` in the file and `.text` in a script, and a
//! card's artwork must be the size of the whole deck or it does not appear.

#![warn(missing_docs)]

mod deck;
mod image;
mod lil;

pub use deck::deck;

/// A translated deck, and what could not be carried across.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Translation {
    /// The `.deck` file.
    pub source: String,
    /// One line for everything that did not translate, in the order met.
    pub notes: Vec<String>,
}

impl Translation {
    /// Whether everything translated.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.notes.is_empty()
    }
}
