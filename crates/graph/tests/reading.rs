//! Reading a stack's shape out of its scripts.

use hyperlab_graph::{Destination, Graph};
use hyperlab_runtime::{Command, PartOwner, Runtime};
use hyperlab_stack::{Id, Object, ObjectId, ObjectKind, PartKind, Rect, Stack};

/// A stack of `cards` cards, all on one background.
fn stack_of(cards: usize) -> Runtime {
    let mut runtime = Runtime::new(Stack::new("Test"));
    for index in 0..cards.saturating_sub(1) {
        runtime
            .execute(Command::CreateCard {
                after: index,
                background: None,
            })
            .unwrap();
    }
    runtime
}

fn name_card(runtime: &mut Runtime, index: usize, name: &str) -> Id {
    let id = runtime.stack().cards()[index].id();
    runtime
        .execute(Command::Rename {
            object: ObjectId::new(ObjectKind::Card, id),
            name: name.into(),
        })
        .unwrap();
    id
}

fn script_on(runtime: &mut Runtime, object: ObjectId, body: &str) {
    runtime
        .execute(Command::SetScript {
            object,
            script: format!("on mouseUp\n{body}\nend mouseUp"),
        })
        .unwrap();
}

fn card_script(runtime: &mut Runtime, index: usize, body: &str) {
    let id = runtime.stack().cards()[index].id();
    script_on(runtime, ObjectId::new(ObjectKind::Card, id), body);
}

/// Where the one edge out of card `index` leads.
fn only_edge_from(graph: &Graph, from: Id) -> &Destination {
    let out: Vec<_> = graph
        .edges
        .iter()
        .filter(|edge| edge.from == from)
        .collect();
    assert_eq!(out.len(), 1, "expected one way out of {from}, got {out:?}");
    &out[0].to
}

#[test]
fn a_card_named_in_a_script_becomes_an_edge_to_that_card() {
    let mut runtime = stack_of(2);
    let home = name_card(&mut runtime, 0, "Home");
    let library = name_card(&mut runtime, 1, "Library");
    card_script(&mut runtime, 0, r#"go to card "Library""#);

    let graph = Graph::of(runtime.stack());
    assert_eq!(
        only_edge_from(&graph, home),
        &Destination::Card { id: library }
    );
}

#[test]
fn a_name_is_matched_however_it_was_capitalised() {
    let mut runtime = stack_of(2);
    let home = name_card(&mut runtime, 0, "Home");
    let library = name_card(&mut runtime, 1, "Library");
    card_script(&mut runtime, 0, r#"go to card "LIBRARY""#);

    let graph = Graph::of(runtime.stack());
    assert_eq!(
        only_edge_from(&graph, home),
        &Destination::Card { id: library }
    );
}

#[test]
fn next_and_previous_are_resolved_against_the_card_you_are_on() {
    // The reason an edge is worked out per card rather than per script: the
    // same `go to next card` means somewhere different from each one.
    let mut runtime = stack_of(3);
    let ids: Vec<Id> = runtime.stack().cards().iter().map(Object::id).collect();
    let background = runtime.stack().cards()[0].background();
    script_on(
        &mut runtime,
        ObjectId::new(ObjectKind::Background, background),
        "go to next card",
    );

    let graph = Graph::of(runtime.stack());
    assert_eq!(
        only_edge_from(&graph, ids[0]),
        &Destination::Card { id: ids[1] }
    );
    assert_eq!(
        only_edge_from(&graph, ids[1]),
        &Destination::Card { id: ids[2] }
    );
    // And it wraps, exactly as the runtime does.
    assert_eq!(
        only_edge_from(&graph, ids[2]),
        &Destination::Card { id: ids[0] }
    );
}

#[test]
fn a_button_on_a_background_leads_off_every_card_that_shares_it() {
    let mut runtime = stack_of(3);
    let background = runtime.stack().cards()[0].background();
    let button = runtime
        .execute(Command::CreatePart {
            owner: PartOwner::Background { id: background },
            kind: PartKind::Button,
            name: "Home".into(),
            geometry: Rect::new(0, 0, 60, 20),
        })
        .unwrap()
        .unwrap();
    script_on(&mut runtime, button, "go to first card");

    let graph = Graph::of(runtime.stack());
    assert_eq!(
        graph.edges.len(),
        3,
        "one per card, because that is what clicking it does"
    );
    assert!(graph.edges.iter().all(|edge| edge.via == button));
}

#[test]
fn a_go_inside_an_if_is_still_a_way_out() {
    let mut runtime = stack_of(2);
    let home = name_card(&mut runtime, 0, "Home");
    let second = runtime.stack().cards()[1].id();
    card_script(
        &mut runtime,
        0,
        "if the short date is not empty then\n  go to last card\nend if",
    );

    let graph = Graph::of(runtime.stack());
    assert_eq!(
        only_edge_from(&graph, home),
        &Destination::Card { id: second }
    );
}

#[test]
fn a_destination_that_only_running_it_would_settle_says_so() {
    let mut runtime = stack_of(2);
    let home = name_card(&mut runtime, 0, "Home");
    card_script(&mut runtime, 0, "go to card whicheverTheyPicked");

    let graph = Graph::of(runtime.stack());
    assert!(
        matches!(only_edge_from(&graph, home), Destination::Unresolved { .. }),
        "a variable destination must not be guessed at"
    );
    assert_eq!(graph.unresolved(), 1);
}

#[test]
fn a_link_to_a_card_that_is_not_there_is_reported_rather_than_dropped() {
    // The bug this whole thing is worth building for: someone deleted the
    // card, and nothing said so.
    let mut runtime = stack_of(1);
    let home = name_card(&mut runtime, 0, "Home");
    card_script(&mut runtime, 0, r#"go to card "Deleted Last Tuesday""#);

    let graph = Graph::of(runtime.stack());
    assert_eq!(
        only_edge_from(&graph, home),
        &Destination::Missing {
            wanted: "card \"Deleted Last Tuesday\"".into()
        }
    );
    assert_eq!(graph.broken().len(), 1);
}

#[test]
fn go_back_is_a_route_but_not_to_anywhere_in_particular() {
    let mut runtime = stack_of(2);
    let home = name_card(&mut runtime, 0, "Home");
    card_script(&mut runtime, 0, "go back");

    let graph = Graph::of(runtime.stack());
    assert_eq!(only_edge_from(&graph, home), &Destination::Back);
    assert!(graph.node(home).unwrap().leads_anywhere);
}

#[test]
fn a_card_nothing_leads_to_is_named() {
    let mut runtime = stack_of(3);
    let ids: Vec<Id> = runtime.stack().cards().iter().map(Object::id).collect();
    // First goes to third. Nothing mentions the second at all.
    card_script(&mut runtime, 0, "go to last card");

    let graph = Graph::of(runtime.stack());
    let orphans: Vec<Id> = graph.unreachable().iter().map(|node| node.id).collect();
    assert_eq!(orphans, vec![ids[1]]);
}

#[test]
fn the_first_card_is_reachable_because_that_is_where_you_start() {
    let graph = Graph::of(stack_of(2).stack());
    let first = graph.nodes[0].id;
    assert!(graph.node(first).unwrap().reachable);
    assert_eq!(graph.unreachable().len(), 1, "only the second is stranded");
}

#[test]
fn the_only_card_in_a_stack_is_not_a_trap() {
    // There is nowhere else to be, so there is nothing to warn about.
    let graph = Graph::of(stack_of(1).stack());
    assert!(graph.dead_ends().is_empty());
}

#[test]
fn a_card_with_no_way_out_is_a_dead_end_and_a_self_loop_does_not_count() {
    let mut runtime = stack_of(2);
    let home = name_card(&mut runtime, 0, "Home");
    card_script(&mut runtime, 0, "go to this card");

    let graph = Graph::of(runtime.stack());
    assert!(
        !graph.node(home).unwrap().leads_anywhere,
        "going nowhere is not a way out"
    );
    assert_eq!(graph.dead_ends().len(), 2);
}

#[test]
fn a_script_that_does_not_parse_is_skipped_rather_than_panicking() {
    let mut runtime = stack_of(1);
    let home = runtime.stack().cards()[0].id();
    // Straight past the command bus: the editor would refuse this, but a
    // hand-edited .hl file would not.
    runtime
        .stack_mut_unchecked()
        .card_mut(home)
        .unwrap()
        .set_script("on mouseUp\n  repeat");

    let graph = Graph::of(runtime.stack());
    assert!(graph.edges.is_empty());
    assert_eq!(graph.node(home).unwrap().position, 1);
}

#[test]
fn cards_are_grouped_by_the_background_they_share() {
    let mut runtime = stack_of(2);
    let second = runtime
        .execute(Command::CreateBackground {
            name: "Other".into(),
        })
        .unwrap()
        .unwrap();
    let moved = runtime.stack().cards()[1].id();
    runtime
        .stack_mut_unchecked()
        .card_mut(moved)
        .unwrap()
        .set_background(second.id);

    let graph = Graph::of(runtime.stack());
    assert_eq!(graph.by_background().len(), 2);
}

#[test]
fn the_dot_output_names_every_card_and_marks_what_is_uncertain() {
    let mut runtime = stack_of(2);
    name_card(&mut runtime, 0, "Home");
    name_card(&mut runtime, 1, "Library");
    card_script(&mut runtime, 0, r#"go to card "Library""#);
    card_script(&mut runtime, 1, "go to card wherever");

    let dot = hyperlab_graph::to_dot(&Graph::of(runtime.stack()));

    assert!(dot.contains("digraph \"Test\" {"));
    assert!(dot.contains("1. Home"), "got {dot}");
    assert!(dot.contains("2. Library"), "got {dot}");
    assert!(dot.contains("style=solid"), "the certain route");
    assert!(dot.contains("style=dashed"), "the uncertain one");
    assert!(dot.contains("subgraph cluster_"), "grouped by background");
}
