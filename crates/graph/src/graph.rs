//! The graph itself: what a stack looks like when you draw the routes.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use hyperlab_stack::{Id, Object, ObjectId, Stack};
use serde::Serialize;

/// Where a `go` statement leads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Destination {
    /// A card, known for certain.
    Card {
        /// Which one.
        id: Id,
    },
    /// `go back`, which depends on where the reader has been.
    Back,
    /// Written plainly, but there is no such card.
    ///
    /// Worth drawing rather than dropping: a link to a card somebody deleted
    /// is a bug, and an invisible one.
    Missing {
        /// What the script asked for.
        wanted: String,
    },
    /// Only running the handler would say.
    Unresolved {
        /// Why not, in words.
        because: String,
    },
}

impl Destination {
    /// A card, if this leads somewhere certain.
    #[must_use]
    pub const fn card(&self) -> Option<Id> {
        match self {
            Self::Card { id } => Some(*id),
            _ => None,
        }
    }

    // Constructors used by the walker, which thinks in plain values.

    pub(crate) const fn card_at(id: Id) -> Self {
        Self::Card { id }
    }

    pub(crate) fn missing(wanted: impl Into<String>) -> Self {
        Self::Missing {
            wanted: wanted.into(),
        }
    }

    pub(crate) fn unresolved(because: impl Into<String>) -> Self {
        Self::Unresolved {
            because: because.into(),
        }
    }
}

/// One way out of one card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    /// The card you are standing on.
    pub from: Id,
    /// Where the statement would take you.
    pub to: Destination,
    /// The object whose script says so — a button, the card, the background.
    pub via: ObjectId,
    /// The line of that script, for jumping to it.
    pub line: u32,
}

/// One card, and what the graph knows about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    /// The card.
    pub id: Id,
    /// Its name.
    pub name: String,
    /// Its place in the stack, counting from one.
    pub position: usize,
    /// The background it is drawn on, which is how the picture is grouped.
    pub background: Id,
    /// Whether anything leads here from the first card.
    pub reachable: bool,
    /// Whether anything leads away from here.
    pub leads_anywhere: bool,
}

/// A stack, read as routes between cards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Graph {
    /// The name of the stack this came from.
    pub stack: String,
    /// Every card, in stack order.
    pub nodes: Vec<Node>,
    /// Every `go` found in every script that a card can reach.
    pub edges: Vec<Edge>,
}

impl Graph {
    /// Reads a stack.
    ///
    /// Every card is a node. Every `go` in any script that card's messages
    /// can reach is an edge out of it — so a button on a shared background
    /// produces one edge per card that shares it, which is what actually
    /// happens when the reader clicks it.
    #[must_use]
    pub fn of(stack: &Stack) -> Self {
        let mut edges = Vec::new();
        for card in stack.cards() {
            for (owner, script) in crate::walk::scripts_reachable_from(stack, card) {
                crate::walk::edges_in(stack, &script, owner, card.id(), &mut edges);
            }
        }

        let reachable = reachable_from(stack.cards().first().map(Object::id), &edges);
        let leaves: BTreeSet<Id> = edges
            .iter()
            .filter(|edge| !matches!(edge.to, Destination::Card { id } if id == edge.from))
            .map(|edge| edge.from)
            .collect();

        let nodes = stack
            .cards()
            .iter()
            .enumerate()
            .map(|(index, card)| Node {
                id: card.id(),
                name: card.name().to_string(),
                position: index + 1,
                background: card.background(),
                reachable: reachable.contains(&card.id()),
                leads_anywhere: leaves.contains(&card.id()),
            })
            .collect();

        Self {
            stack: stack.name().to_string(),
            nodes,
            edges,
        }
    }

    /// Cards nothing leads to from the first one.
    ///
    /// The most useful thing the graph knows. A stack grows by copying cards
    /// and rewiring buttons, and an orphan is invisible until someone goes
    /// looking for a card that no longer has a way in.
    #[must_use]
    pub fn unreachable(&self) -> Vec<&Node> {
        self.nodes.iter().filter(|node| !node.reachable).collect()
    }

    /// Cards with no way out, which trap a reader who arrives.
    ///
    /// A stack of one card is never a trap: there is nowhere else to be.
    #[must_use]
    pub fn dead_ends(&self) -> Vec<&Node> {
        if self.nodes.len() < 2 {
            return Vec::new();
        }
        self.nodes
            .iter()
            .filter(|node| !node.leads_anywhere)
            .collect()
    }

    /// Links to cards that are not there.
    #[must_use]
    pub fn broken(&self) -> Vec<&Edge> {
        self.edges
            .iter()
            .filter(|edge| matches!(edge.to, Destination::Missing { .. }))
            .collect()
    }

    /// How many routes could not be read without running them.
    #[must_use]
    pub fn unresolved(&self) -> usize {
        self.edges
            .iter()
            .filter(|edge| matches!(edge.to, Destination::Unresolved { .. } | Destination::Back))
            .count()
    }

    /// The node for a card.
    #[must_use]
    pub fn node(&self, card: Id) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == card)
    }

    /// Cards grouped by the background they share, in stack order.
    ///
    /// What gives the picture its shape: the Myst graph colours by Age, and a
    /// background is HyperCard's Age.
    #[must_use]
    pub fn by_background(&self) -> BTreeMap<Id, Vec<&Node>> {
        let mut grouped: BTreeMap<Id, Vec<&Node>> = BTreeMap::new();
        for node in &self.nodes {
            grouped.entry(node.background).or_default().push(node);
        }
        grouped
    }
}

/// Every card you can get to from `start` by following certain edges.
fn reachable_from(start: Option<Id>, edges: &[Edge]) -> BTreeSet<Id> {
    let mut seen = BTreeSet::new();
    let Some(start) = start else {
        return seen;
    };

    let mut outward: BTreeMap<Id, Vec<Id>> = BTreeMap::new();
    for edge in edges {
        if let Some(target) = edge.to.card() {
            outward.entry(edge.from).or_default().push(target);
        }
    }

    let mut queue = VecDeque::from([start]);
    seen.insert(start);
    while let Some(card) = queue.pop_front() {
        for &next in outward.get(&card).into_iter().flatten() {
            if seen.insert(next) {
                queue.push_back(next);
            }
        }
    }
    seen
}
