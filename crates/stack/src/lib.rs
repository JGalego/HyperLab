//! The HyperLab object model.
//!
//! This crate is the foundation every other crate is built on. It describes
//! *what* a stack is; it never describes how a stack is edited (that is the
//! runtime's job), how it is stored (persistence), or how it is drawn (the
//! renderer).
//!
//! # The shape of a stack
//!
//! ```text
//! Stack
//!     Background
//!         Part (button / field)
//!     Card
//!         Part (button / field)
//! ```
//!
//! Every object shares the same [`ObjectCore`]: an [`Id`], a name, a script,
//! a [`PropertyBag`] and timestamps. Buttons and fields are both [`Part`]s;
//! they differ only by their [`PartKind`] and their default properties. That
//! uniformity is deliberate — a new kind of part should not require a new
//! kind of object.
//!
//! # Example
//!
//! ```
//! use hyperlab_stack::{Object, PartContainer, PartKind, Rect, Stack, Value};
//!
//! let mut stack = Stack::new("Address Book");
//! let card_id = stack.cards()[0].id();
//!
//! let button = stack.new_part(PartKind::Button, "Next", Rect::new(20, 20, 80, 24));
//! let button_id = button.id();
//! stack.card_mut(card_id).unwrap().add_part(button);
//!
//! let card = stack.card(card_id).unwrap();
//! assert_eq!(card.part(button_id).unwrap().name(), "Next");
//! assert_eq!(card.part(button_id).unwrap().property("visible"), Some(Value::Bool(true)));
//! ```

#![warn(missing_docs)]

mod background;
mod card;
mod container;
mod error;
mod geometry;
mod id;
mod object;
mod part;
mod property;
mod stack;
mod time;
mod value;

pub use background::Background;
pub use card::Card;
pub use container::PartContainer;
pub use error::{StackError, StackResult};
pub use geometry::{Point, Rect, Size};
pub use id::{Id, IdGenerator};
pub use object::{Object, ObjectCore, ObjectId, ObjectKind};
pub use part::{Part, PartKind};
pub use property::PropertyBag;
pub use stack::{PartLocation, Stack, centre_of};
pub use time::now_millis;
pub use value::Value;
