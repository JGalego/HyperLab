//! A stack as a document.
//!
//! [`to_pdf`] writes one page per card, the size of the card, with the parts
//! drawn where they sit and the stack's pictures placed as vector artwork.
//! Text is real text — Helvetica, one of the fourteen fonts a PDF reader
//! supplies itself — so an exported card can be searched and copied out of
//! rather than only looked at.
//!
//! ```no_run
//! let stack = hyperlab_persistence::load("examples/Todo.hl")?;
//! std::fs::write("Todo.pdf", hyperlab_export::to_pdf(&stack)?)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # What this is not
//!
//! It is not the renderer. The desktop draws a card with HTML and CSS and a
//! browser's idea of a line break; this draws the same objects with a PDF's.
//! They agree on where every part sits, what it says and what it looks like,
//! and they will differ by a pixel on where a long line wraps. Chasing that
//! would mean shipping the browser, so the two are kept honest about being
//! two renderings of one model rather than one rendering copied twice.
//!
//! The map is exported by the desktop instead, and for the same reason in
//! reverse: a card is in the model, but the shape of a map is a layout the
//! renderer worked out, and only the renderer has it.

#![warn(missing_docs)]

mod art;
mod fonts;
mod metrics;
mod page;

use hyperlab_stack::Stack;

pub use fonts::add_font;
pub use metrics::{encode, wrap};

/// Why a stack could not be written out.
#[derive(Debug)]
pub enum ExportError {
    /// A picture in the stack could not be read as a drawing.
    ///
    /// Carries the picture's name, because a stack with forty of them needs to
    /// say which one.
    Picture {
        /// The picture's name in the stack's library.
        name: String,
        /// What the converter made of it.
        reason: String,
    },
    /// The document was built and could not be assembled.
    Document(String),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Picture { name, reason } => {
                write!(out, "the picture \"{name}\" could not be drawn: {reason}")
            }
            Self::Document(reason) => write!(out, "the document could not be written: {reason}"),
        }
    }
}

impl std::error::Error for ExportError {}

/// Writes the whole stack as a PDF, one page per card.
///
/// # Errors
///
/// Returns [`ExportError::Picture`] naming the picture that could not be
/// converted. A stack with no pictures cannot fail.
pub fn to_pdf(stack: &Stack) -> Result<Vec<u8>, ExportError> {
    page::document(stack)
}
