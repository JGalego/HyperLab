//! The shape of a stack: which cards lead where.
//!
//! A stack is a graph pretending to be a pile of paper. Every `go` in every
//! script is an edge, and drawing them is how you see what you have actually
//! built — which cards nobody can reach, which trap the reader, which links
//! point at a card that was deleted last week.
//!
//! [Guillaume Lethuillier's graph of Myst][myst] does this from the outside:
//! crack a 1993 stack open with `stackimport`, recover the HyperTalk, parse
//! it, and draw 1,355 cards. HyperLab does it from the inside, because it
//! already owns the parser — so the graph is a pure function of the stack and
//! is never out of date.
//!
//! [myst]: https://glthr.com/myst-graph-1
//!
//! # What it can and cannot know
//!
//! Nothing here runs. `go to card "Library"` is certain, and so is `go to
//! next card` once you know which card you are standing on. `go to card
//! whicheverOneTheyPicked` is not, and is reported as
//! [`Destination::Unresolved`] rather than guessed at or quietly dropped.
//! That distinction is the interesting part of the drawing, not a shortfall
//! in it: the same limit shows up in the Myst graph as its dashed edges.
//!
//! ```
//! use hyperlab_graph::Graph;
//! use hyperlab_runtime::{Command, Runtime};
//! use hyperlab_stack::{ObjectId, ObjectKind, Stack};
//!
//! let mut runtime = Runtime::new(Stack::new("Tour"));
//! let first = runtime.current_card();
//! runtime
//!     .execute(Command::CreateCard { after: 0, background: None })
//!     .unwrap();
//! runtime
//!     .execute(Command::SetScript {
//!         object: ObjectId::new(ObjectKind::Card, first),
//!         script: "on mouseUp\n  go to next card\nend mouseUp".into(),
//!     })
//!     .unwrap();
//!
//! let graph = Graph::of(runtime.stack());
//! assert_eq!(graph.nodes.len(), 2);
//! assert_eq!(graph.edges.len(), 1);
//!
//! // The second card has no way out, and nothing leads back to the first.
//! assert_eq!(graph.dead_ends().len(), 1);
//! ```

#![warn(missing_docs)]

mod dot;
mod graph;
mod walk;

pub use dot::to_dot;
pub use graph::{Destination, Edge, Graph, Node};
