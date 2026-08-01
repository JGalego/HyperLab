//! Finding every `go` in a stack, and working out where it leads.
//!
//! This is a *static* reading. It never runs anything, so it sees exactly
//! what is written and no more — which is the honest limit of the technique
//! and the reason [`Destination::Unresolved`] exists rather than a guess.

use hyperlab_parser::{
    ast::{Destination as Written, Expr, Ordinal, Script, Specifier, StatementKind},
    parse,
};
use hyperlab_stack::{Id, Object, ObjectId, PartContainer, Stack};

use crate::graph::{Destination, Edge};

/// Every navigation a script could perform from `from`.
///
/// `me` is the object whose script this is, and `from` the card the reader is
/// standing on — the two differ for a background or stack script, which is
/// exactly why `next card` cannot be resolved without knowing both.
pub(crate) fn edges_in(stack: &Stack, source: &str, me: ObjectId, from: Id, into: &mut Vec<Edge>) {
    let Ok(script) = parse(source) else {
        // A script that does not parse cannot navigate anywhere either; the
        // editor already tells the author about it.
        return;
    };
    for (destination, line) in gos(&script, me.kind) {
        into.push(Edge {
            from,
            to: resolve(stack, &destination, from),
            via: me,
            line,
        });
    }
}

/// Messages the stack receives directly rather than up the path from a card.
///
/// `on openStack` runs once, when the stack is opened. Counting the `go` in
/// it as a way out of *every* card is what a naive message-path walk does,
/// and it is wrong in a way that quietly matters: almost every stack starts
/// with `on openStack go to first card`, so every card would look like it
/// led somewhere and nothing would ever be reported as a dead end.
const STACK_ONLY: [&str; 2] = ["openstack", "closestack"];

/// Every `go` in a script that a card could actually set off, with the line
/// it was written on.
///
/// Walks into `if` and `repeat` bodies: a `go` inside a branch is still a way
/// out of the card, and drawing only the unconditional ones would understate
/// the graph badly.
fn gos(script: &Script, owner: hyperlab_stack::ObjectKind) -> Vec<(Written, u32)> {
    fn walk(block: &[hyperlab_parser::ast::Statement], found: &mut Vec<(Written, u32)>) {
        for statement in block {
            match &statement.kind {
                StatementKind::Go(destination) => {
                    found.push((destination.clone(), statement.line));
                }
                StatementKind::If {
                    branches,
                    otherwise,
                } => {
                    for branch in branches {
                        walk(&branch.body, found);
                    }
                    if let Some(body) = otherwise {
                        walk(body, found);
                    }
                }
                StatementKind::Repeat { body, .. } => walk(body, found),
                _ => {}
            }
        }
    }

    let mut found = Vec::new();
    for handler in &script.handlers {
        if owner == hyperlab_stack::ObjectKind::Stack
            && STACK_ONLY.contains(&handler.name.to_ascii_lowercase().as_str())
        {
            continue;
        }
        walk(&handler.body, &mut found);
    }
    found
}

/// Works out which card a written destination means, if it can be known.
fn resolve(stack: &Stack, written: &Written, from: Id) -> Destination {
    let specifier = match written {
        // Where `go back` lands depends on where you have been, which is a
        // property of the visit rather than of the stack.
        Written::Back => return Destination::Back,
        Written::Card(specifier) => specifier,
    };

    match specifier {
        Specifier::Current => Destination::card_at(from),
        Specifier::Id(expression) => match literal_number(expression) {
            Some(number) => card_by_id(stack, Id::new(number as u64)),
            None => Destination::unresolved("an id worked out as it runs"),
        },
        Specifier::Value(expression) => resolve_value(stack, expression),
        Specifier::Ordinal(ordinal) => resolve_ordinal(stack, *ordinal, from),
    }
}

/// `go to card "Home"`, `go to card 3`, `go to card someVariable`.
fn resolve_value(stack: &Stack, expression: &Expr) -> Destination {
    match expression {
        Expr::Text(name) => card_by_name(stack, name),
        Expr::Number(position) => card_at(stack, *position as isize - 1),
        // A bare word is a variable if it was ever set and a string if it was
        // not, and only running the handler settles which.
        Expr::Variable(name) => {
            Destination::unresolved(format!("\"{name}\", which may be a variable"))
        }
        _ => Destination::unresolved("a card worked out as it runs"),
    }
}

fn resolve_ordinal(stack: &Stack, ordinal: Ordinal, from: Id) -> Destination {
    let count = stack.card_count();
    if count == 0 {
        return Destination::unresolved("an empty stack");
    }

    match ordinal {
        Ordinal::First => card_at(stack, 0),
        Ordinal::Second => card_at(stack, 1),
        Ordinal::Third => card_at(stack, 2),
        Ordinal::Fourth => card_at(stack, 3),
        Ordinal::Fifth => card_at(stack, 4),
        Ordinal::Last => card_at(stack, count as isize - 1),
        Ordinal::Middle => card_at(stack, count as isize / 2),
        // Relative to wherever the reader is, which is knowable — this is the
        // whole reason an edge is computed per card rather than per script.
        Ordinal::Next => step(stack, from, 1),
        Ordinal::Previous => step(stack, from, -1),
        Ordinal::Any => Destination::unresolved("any card, chosen at random"),
    }
}

/// The card `by` places along from `from`, wrapping as the runtime does.
fn step(stack: &Stack, from: Id, by: isize) -> Destination {
    let Some(index) = stack.card_index(from) else {
        return Destination::unresolved("a card that is no longer there");
    };
    let count = stack.card_count() as isize;
    card_at(stack, (index as isize + by).rem_euclid(count))
}

fn card_at(stack: &Stack, index: isize) -> Destination {
    if index < 0 {
        return Destination::missing(format!("card {}", index + 1));
    }
    stack.cards().get(index as usize).map_or_else(
        || Destination::missing(format!("card {}", index + 1)),
        |card| Destination::card_at(card.id()),
    )
}

fn card_by_id(stack: &Stack, id: Id) -> Destination {
    stack.card(id).map_or_else(
        || Destination::missing(format!("card id {id}")),
        |card| Destination::card_at(card.id()),
    )
}

fn card_by_name(stack: &Stack, name: &str) -> Destination {
    stack
        .cards()
        .iter()
        .find(|card| card.name().eq_ignore_ascii_case(name))
        .map_or_else(
            || Destination::missing(format!("card \"{name}\"")),
            |card| Destination::card_at(card.id()),
        )
}

/// A number written in the source, rather than computed from one.
fn literal_number(expression: &Expr) -> Option<f64> {
    match expression {
        Expr::Number(number) => Some(*number),
        _ => None,
    }
}

/// Every script that a message from `card` could reach, with its owner.
///
/// The message path, in the order the runtime walks it: the card's own parts,
/// the card, the background's parts, the background, the stack. A `go` in any
/// of them is a way off this card.
pub(crate) fn scripts_reachable_from(
    stack: &Stack,
    card: &hyperlab_stack::Card,
) -> Vec<(ObjectId, String)> {
    let mut scripts = Vec::new();
    let mut take = |object: ObjectId, script: &str| {
        if !script.trim().is_empty() {
            scripts.push((object, script.to_string()));
        }
    };

    for part in card.parts() {
        take(part.object_id(), part.script());
    }
    take(card.object_id(), card.script());

    if let Some(background) = stack.background(card.background()) {
        for part in background.parts() {
            take(part.object_id(), part.script());
        }
        take(background.object_id(), background.script());
    }
    take(stack.object_id(), stack.script());

    scripts
}
