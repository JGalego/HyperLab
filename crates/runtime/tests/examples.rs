//! The example stacks are part of the test suite.
//!
//! An example that no longer works is worse than no example, so every bundle
//! in `examples/` is loaded here, every script in it is parsed, and the
//! buttons that do something interesting are actually clicked.

use std::path::{Path, PathBuf};

use hyperlab_persistence::load;
use hyperlab_runtime::{Message, Runtime};
use hyperlab_stack::{Object, ObjectId, PartContainer, PartKind, Stack, Value};

fn examples_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate is two levels below the repository root")
        .join("examples")
}

fn open(name: &str) -> Runtime {
    let path = examples_directory().join(format!("{name}.hl"));
    let stack = load(&path).unwrap_or_else(|error| panic!("could not open {name}: {error}"));
    let mut runtime = Runtime::new(stack);
    runtime
        .open_stack()
        .unwrap_or_else(|error| panic!("opening {name} failed: {error}"));
    runtime.take_effects();
    runtime
}

/// Clicks the named button, wherever it is.
fn click(runtime: &mut Runtime, name: &str) {
    let card = runtime.current_card();
    let button = runtime
        .stack()
        .card(card)
        .and_then(|card| card.part_named(PartKind::Button, name))
        .map(Object::object_id)
        .or_else(|| {
            runtime
                .stack()
                .background_of(card)
                .and_then(|background| background.part_named(PartKind::Button, name))
                .map(Object::object_id)
        })
        .unwrap_or_else(|| panic!("there is no button named \"{name}\""));

    runtime
        .send_message(&Message::new("mouseUp"), button)
        .unwrap_or_else(|error| panic!("clicking \"{name}\" failed: {error}"));
}

fn field_text(runtime: &Runtime, name: &str) -> String {
    let card = runtime.current_card();
    runtime
        .stack()
        .card(card)
        .and_then(|card| card.part_named(PartKind::Field, name))
        .unwrap_or_else(|| panic!("there is no field named \"{name}\""))
        .property("text")
        .unwrap_or(Value::Empty)
        .as_text()
}

#[test]
fn every_example_loads_and_every_script_in_it_parses() {
    let directory = examples_directory();
    let mut checked = 0;

    for entry in std::fs::read_dir(&directory).expect("examples/ should exist") {
        let path = entry.expect("the directory should be readable").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("hl") {
            continue;
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let stack = load(&path).unwrap_or_else(|error| panic!("{name}: {error}"));

        for (owner, source) in scripts(&stack) {
            Runtime::check_script(&source)
                .unwrap_or_else(|error| panic!("{name}, script of {owner}: {error}"));
        }
        checked += 1;
    }

    assert!(
        checked >= 3,
        "expected the three example stacks, found {checked}"
    );
}

/// Every script in a stack, with a description of where it came from.
fn scripts(stack: &Stack) -> Vec<(String, String)> {
    let mut found = vec![(
        format!("stack \"{}\"", stack.name()),
        stack.script().to_string(),
    )];
    for background in stack.backgrounds() {
        found.push((
            format!("background \"{}\"", background.name()),
            background.script().to_string(),
        ));
        found.extend(background.parts().iter().map(|part| {
            (
                format!("{} \"{}\"", part.kind(), part.name()),
                part.script().to_string(),
            )
        }));
    }
    for card in stack.cards() {
        found.push((
            format!("card \"{}\"", card.name()),
            card.script().to_string(),
        ));
        found.extend(card.parts().iter().map(|part| {
            (
                format!("{} \"{}\"", part.kind(), part.name()),
                part.script().to_string(),
            )
        }));
    }
    found
        .into_iter()
        .filter(|(_, source)| !source.trim().is_empty())
        .collect()
}

#[test]
fn the_address_book_opens_on_the_first_person_and_can_be_paged_through() {
    let mut runtime = open("Address Book");
    assert_eq!(field_text(&runtime, "Name"), "Ada Lovelace");

    click(&mut runtime, "Next");
    assert_eq!(field_text(&runtime, "Name"), "Grace Hopper");

    click(&mut runtime, "Previous");
    assert_eq!(field_text(&runtime, "Name"), "Ada Lovelace");

    click(&mut runtime, "Previous");
    assert_eq!(
        field_text(&runtime, "Name"),
        "Bill Atkinson",
        "paging back from the first card wraps to the last"
    );
}

#[test]
fn the_address_book_can_count_itself() {
    let mut runtime = open("Address Book");
    runtime.take_effects();
    click(&mut runtime, "How Many");
    assert_eq!(
        runtime.take_effects(),
        vec![hyperlab_runtime::Effect::Answer {
            message: "This book holds 3 people.".into(),
        }]
    );
}

#[test]
fn doubling_a_recipe_doubles_only_the_amounts() {
    let mut runtime = open("Recipe Box");
    assert_eq!(field_text(&runtime, "Title"), "Pancakes");

    click(&mut runtime, "Double It");
    let ingredients = field_text(&runtime, "Ingredients");
    assert!(ingredients.starts_with("400 g flour"), "{ingredients}");
    assert!(ingredients.contains("4 eggs"), "{ingredients}");
    assert!(ingredients.contains("600 ml milk"), "{ingredients}");
    assert!(
        ingredients.contains("2 pinch of salt"),
        "the words after the amount are left alone: {ingredients}"
    );
}

#[test]
fn doubling_a_recipe_can_be_undone() {
    let mut runtime = open("Recipe Box");
    let before = field_text(&runtime, "Ingredients");
    click(&mut runtime, "Double It");
    assert_ne!(field_text(&runtime, "Ingredients"), before);

    assert!(runtime.undo().unwrap());
    assert_eq!(
        field_text(&runtime, "Ingredients"),
        before,
        "a script's edit is undoable like any other"
    );
}

#[test]
fn the_todo_list_counts_what_is_left_and_clears_what_is_done() {
    let mut runtime = open("Todo");
    runtime.take_effects();

    click(&mut runtime, "How Many");
    assert_eq!(
        runtime.take_effects(),
        vec![hyperlab_runtime::Effect::Answer {
            message: "2 left to do.".into(),
        }]
    );

    click(&mut runtime, "Clear Done");
    let items = field_text(&runtime, "Items");
    assert!(
        !items.contains("x read"),
        "finished items are removed: {items}"
    );
    assert!(items.contains("write a stack of my own"), "{items}");
}

#[test]
fn adding_to_the_todo_list_asks_first() {
    let mut runtime = open("Todo");
    runtime.take_effects();
    click(&mut runtime, "Add");

    // The default host cancels every question, so nothing is added — which is
    // the behaviour a script must cope with.
    assert!(matches!(
        runtime.take_effects().first(),
        Some(hyperlab_runtime::Effect::Ask { .. })
    ));
    assert!(!field_text(&runtime, "Items").is_empty());
}

#[test]
fn examples_keep_working_after_a_save_and_reload() {
    let stack = load(examples_directory().join("Todo.hl")).expect("Todo.hl should load");
    let temporary =
        std::env::temp_dir().join(format!("hyperlab-example-{}.hl", std::process::id()));
    hyperlab_persistence::save(&temporary, &stack).expect("it should save");
    let reloaded = load(&temporary).expect("it should load again");
    std::fs::remove_dir_all(&temporary).ok();

    assert_eq!(reloaded, stack);
    let _: ObjectId = ObjectId::new(reloaded.kind(), reloaded.id());
}

// -------------------------------------------------------------- the mansion

/// Goes to a card by name, the way a navigation button would.
fn go_to(runtime: &mut Runtime, name: &str) {
    let card = runtime
        .stack()
        .card_named(name)
        .map(Object::id)
        .unwrap_or_else(|| panic!("there is no card named \"{name}\""));
    runtime.go_to_card(card).expect("the card was just found");
}

/// Clicks a part of any kind, so that a picture can be clicked too.
fn click_part(runtime: &mut Runtime, kind: PartKind, name: &str) {
    let card = runtime.current_card();
    let part = runtime
        .stack()
        .card(card)
        .and_then(|card| card.part_named(kind, name))
        .map(Object::object_id)
        .unwrap_or_else(|| panic!("there is no {kind:?} named \"{name}\" on this card"));
    runtime
        .send_message(&Message::new("mouseUp"), part)
        .unwrap_or_else(|error| panic!("clicking \"{name}\" failed: {error}"));
}

#[test]
fn cluedo_carries_its_own_artwork() {
    let runtime = open("Cluedo");
    let stack = runtime.stack();
    assert_eq!(
        stack.images().len(),
        13,
        "the board, six people, six weapons"
    );
    assert!(
        stack.unused_images().is_empty(),
        "every picture should be drawn by something: {:?}",
        stack.unused_images()
    );

    // Every image part names a picture the stack actually has.
    for part in stack
        .parts()
        .filter(|part| part.part_kind() == PartKind::Image)
    {
        let source = part.property("source").unwrap_or(Value::Empty).as_text();
        assert!(
            stack.image(&source).is_some(),
            "image \"{}\" points at \"{source}\", which is not in the bundle",
            part.name()
        );
    }
}

#[test]
fn clicking_a_portrait_names_the_suspect() {
    // The reason a picture is a part: the choice is the picture itself, with
    // no invisible button laid over the top.
    let mut runtime = open("Cluedo");
    go_to(&mut runtime, "Suspects");
    click_part(&mut runtime, PartKind::Image, "Professor Plum");

    // Its script goes back to the board, which is where the answer lands.
    assert_eq!(field_text(&runtime, "Suspect"), "Professor Plum");
}

#[test]
fn the_game_scores_a_suggestion_and_closes_the_case() {
    let mut runtime = open("Cluedo");

    // Nothing chosen yet: it should say so rather than score an empty guess.
    click(&mut runtime, "Ask");
    assert!(
        runtime
            .take_effects()
            .iter()
            .any(|effect| format!("{effect:?}").contains("Name a suspect")),
        "an empty suggestion should be refused"
    );

    go_to(&mut runtime, "Suspects");
    click_part(&mut runtime, PartKind::Image, "Mrs White");
    go_to(&mut runtime, "Weapons");
    click_part(&mut runtime, PartKind::Image, "Lead Pipe");
    click(&mut runtime, "Study");
    click(&mut runtime, "Ask");
    assert!(
        field_text(&runtime, "Replies").contains("1 of 3"),
        "only the weapon is right, got {:?}",
        field_text(&runtime, "Replies")
    );

    go_to(&mut runtime, "Suspects");
    click_part(&mut runtime, PartKind::Image, "Professor Plum");
    click(&mut runtime, "Conservatory");
    click(&mut runtime, "Ask");
    assert!(
        field_text(&runtime, "Replies")
            .starts_with("Professor Plum, Lead Pipe, Conservatory — 3 of 3"),
        "got {:?}",
        field_text(&runtime, "Replies")
    );

    runtime.take_effects();
    click(&mut runtime, "Accuse");
    let said = format!("{:?}", runtime.take_effects());
    assert!(said.contains("Case closed"), "got {said}");

    click(&mut runtime, "Start Over");
    assert_eq!(field_text(&runtime, "Suspect"), "");
    assert_eq!(field_text(&runtime, "Replies"), "");
}
