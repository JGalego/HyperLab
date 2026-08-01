//! The shape of what is written to disk.
//!
//! A saved stack is a *directory*, not a single file:
//!
//! ```text
//! Address Book.hl/
//!     metadata.json          what this is, and which format version
//!     stack.json             the stack itself, and the order of its parts
//!     backgrounds/7.json     one file per background
//!     cards/9.json           one file per card, with its buttons and fields
//!     scripts/card-9.hypertalk
//!     images/study.svg       the pictures the stack carries
//! ```
//!
//! Two decisions are worth explaining, because they cost a little code and
//! buy a lot:
//!
//! * **One file per card.** Two people editing different cards do not
//!   conflict, and a diff of a change shows the card that changed rather
//!   than one enormous line.
//! * **Scripts in their own files.** Code lives in `.hypertalk` files, not
//!   escaped inside JSON strings, so it can be read, diffed, reviewed and
//!   searched with ordinary tools. The JSON describes structure; the
//!   `.hypertalk` files hold behaviour.
//! * **Pictures as pictures.** A `.png` in `images/` opens in an image
//!   viewer and an `.svg` diffs like the text it is. Base64 inside JSON
//!   would have been less code here and worse everywhere else.

use hyperlab_stack::{PropertyBag, Size};
use serde::{Deserialize, Serialize};

/// The format version this build writes.
///
/// Bump this only when the layout changes in a way older builds cannot read.
/// New *properties* never need a bump: unknown properties round-trip.
pub const FORMAT_VERSION: u32 = 1;

/// The file extension of a stack bundle.
pub const BUNDLE_EXTENSION: &str = "hl";

/// `metadata.json`: enough to identify a bundle without loading it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    /// Which version of the layout this bundle uses.
    pub format_version: u32,
    /// The stack's name, so a browser can list bundles cheaply.
    pub name: String,
    /// When it was last written, in milliseconds since the Unix epoch.
    pub saved_at: u64,
    /// How many cards it holds, again for cheap listing.
    pub card_count: usize,
}

/// `stack.json`: the stack's own state, and the order of what it contains.
///
/// The cards and backgrounds themselves live in their own files; this holds
/// only their ids, in order, because order is a property of the stack rather
/// than of any card.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StackDocument {
    /// The stack's id.
    pub id: u64,
    /// Its name.
    pub name: String,
    /// The size of every card in it.
    pub size: Size,
    /// Its properties.
    #[serde(default)]
    pub properties: PropertyBag,
    /// The next id to hand out, so ids stay unique after a reload.
    pub next_id: u64,
    /// When the stack was created.
    #[serde(default)]
    pub created_at: u64,
    /// When it last changed.
    #[serde(default)]
    pub updated_at: u64,
    /// Background ids, in order.
    pub backgrounds: Vec<u64>,
    /// Card ids, in order. This is the order the user flips through.
    pub cards: Vec<u64>,
}

/// The name of the script file belonging to an object.
///
/// Files are named for what they are, so `scripts/` is browsable:
/// `card-9.hypertalk`, `button-14.hypertalk`.
#[must_use]
pub fn script_file_name(kind: hyperlab_stack::ObjectKind, id: hyperlab_stack::Id) -> String {
    format!("{}-{id}.hypertalk", kind.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyperlab_stack::{Id, ObjectKind};

    #[test]
    fn script_files_are_named_after_their_object() {
        assert_eq!(
            script_file_name(ObjectKind::Button, Id::new(14)),
            "button-14.hypertalk"
        );
    }

    #[test]
    fn the_stack_document_uses_camel_case_on_disk() {
        let document = StackDocument {
            id: 1,
            name: "Test".into(),
            size: Size::default(),
            properties: PropertyBag::new(),
            next_id: 4,
            created_at: 0,
            updated_at: 0,
            backgrounds: vec![2],
            cards: vec![3],
        };
        let json = serde_json::to_string(&document).unwrap();
        assert!(json.contains("\"nextId\":4"), "{json}");
    }
}
