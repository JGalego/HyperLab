//! Reading and writing HyperLab stacks.
//!
//! A stack is saved as a `.hl` *bundle*: a directory of small JSON files
//! plus one `.hypertalk` file per script. See [`mod@format`] for the layout and
//! why it is shaped that way.
//!
//! Persistence contains no runtime logic. It turns a [`Stack`](hyperlab_stack::Stack) into files and
//! files back into one, and does nothing else — no defaults beyond
//! what the object model itself supplies, no evaluation, no editing.
//!
//! ```
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use hyperlab_persistence::{load, save};
//! use hyperlab_stack::{Object, Stack};
//!
//! let directory = std::env::temp_dir().join("hyperlab-doc-example.hl");
//! let mut stack = Stack::new("Recipes");
//! stack.set_script("on openStack\n  go to first card\nend openStack");
//!
//! save(&directory, &stack)?;
//! let reloaded = load(&directory)?;
//!
//! assert_eq!(reloaded.name(), "Recipes");
//! assert!(reloaded.script().contains("openStack"));
//! # std::fs::remove_dir_all(&directory).ok();
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

mod error;
pub mod format;
mod load;
pub mod migrate;
mod save;

pub use error::{PersistenceError, PersistenceResult};
pub use format::{BUNDLE_EXTENSION, FORMAT_VERSION, Metadata};
pub use load::{load, load_single_file, read_metadata};
pub use save::{save, save_single_file};
