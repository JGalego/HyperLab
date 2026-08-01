//! Builds the example stacks in `examples/`.
//!
//! The examples are generated rather than hand-written so that they cannot
//! drift out of step with the format: run
//!
//! ```text
//! cargo run -p hyperlab-persistence --example build_examples
//! ```
//!
//! and every bundle is rewritten by the same code that saves a stack in the
//! application. `crates/runtime/tests/examples.rs` then checks that each one
//! loads, that every script in it parses, and that its buttons do what they
//! say they do.
//!
//! # Where things live
//!
//! Backgrounds carry the *layout*: captions and navigation, shared by every
//! card. Cards carry the *data*. A field on a background holds one piece of
//! text for the whole background, which is right for a caption and wrong for
//! a person's name — so the address book's names are card fields.

use std::path::{Path, PathBuf};

use hyperlab_persistence::save;
use hyperlab_stack::{
    Background, Id, Image, Object, Part, PartContainer, PartKind, Rect, Size, Stack, Value,
};

fn main() {
    let root = repository_root();
    let examples = root.join("examples");
    std::fs::create_dir_all(&examples).expect("the examples directory should be writable");

    for (name, mut stack) in [
        ("Address Book", address_book()),
        ("Recipe Box", recipe_box()),
        ("Todo", todo()),
        ("Cluedo", cluedo()),
    ] {
        freeze_timestamps(&mut stack);
        let path = examples.join(format!("{name}.hl"));
        save(&path, &stack).unwrap_or_else(|error| panic!("could not write {name}: {error}"));
        println!("wrote {}", path.display());
    }
}

/// The time every object in an example claims to have been made.
///
/// Real objects are stamped with the clock. The examples are stamped with a
/// constant so that regenerating them produces byte-identical files, which is
/// what lets CI check that `examples/` still matches this program.
const EXAMPLE_TIME: u64 = 1_735_689_600_000; // 2025-01-01T00:00:00Z

/// Gives every object in the stack the same timestamps.
fn freeze_timestamps(stack: &mut Stack) {
    let backgrounds: Vec<Id> = stack.backgrounds().iter().map(Object::id).collect();
    let cards: Vec<Id> = stack.cards().iter().map(Object::id).collect();

    stamp(stack);
    for id in backgrounds {
        let background = stack.background_mut(id).expect("it was just listed");
        stamp(background);
        for part in background.parts_mut() {
            stamp(part);
        }
    }
    for id in cards {
        let card = stack.card_mut(id).expect("it was just listed");
        stamp(card);
        for part in card.parts_mut() {
            stamp(part);
        }
    }
}

fn stamp(object: &mut impl Object) {
    object.core_mut().created_at = EXAMPLE_TIME;
    object.core_mut().updated_at = EXAMPLE_TIME;
}

/// The repository root, found from where Cargo says this crate lives.
fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate is two levels below the repository root")
        .to_path_buf()
}

// ------------------------------------------------------------- the examples

/// A card per person, all drawn on one background: the example that shows
/// what backgrounds are for.
fn address_book() -> Stack {
    let mut stack = Stack::new("Address Book");
    stack.set_size(Size::new(420, 300));
    stack.set_script("on openStack\n  go to first card\nend openStack");

    let background = stack.backgrounds()[0].id();
    rename_background(&mut stack, background, "Person");

    for (caption, top) in [("Name", 24), ("Phone", 56), ("Email", 88), ("Notes", 120)] {
        add_label(&mut stack, background, caption, Rect::new(20, top, 64, 22));
    }

    add_button(
        &mut stack,
        background,
        "Previous",
        Rect::new(20, 248, 84, 24),
        "on mouseUp\n  go to previous card\nend mouseUp",
    );
    add_button(
        &mut stack,
        background,
        "Next",
        Rect::new(112, 248, 84, 24),
        "on mouseUp\n  go to next card\nend mouseUp",
    );
    add_button(
        &mut stack,
        background,
        "How Many",
        Rect::new(300, 248, 96, 24),
        "on mouseUp\n  \
           answer \"This book holds \" & the number of cards & \" people.\"\n\
         end mouseUp",
    );

    let people = [
        (
            "Ada Lovelace",
            "+44 20 7946 0100",
            "ada@analytical.engine",
            "Wrote the first program.",
        ),
        (
            "Grace Hopper",
            "+1 202 555 0143",
            "grace@compiler.example",
            "Found the first bug, and named the rest.",
        ),
        (
            "Bill Atkinson",
            "+1 408 555 0117",
            "bill@hypercard.example",
            "Built HyperCard, which this project is a letter to.",
        ),
    ];

    let mut cards = vec![stack.cards()[0].id()];
    for _ in 1..people.len() {
        let card = stack.new_card(background).expect("the background exists");
        let id = card.id();
        stack.add_card(card);
        cards.push(id);
    }

    for (card, person) in cards.iter().zip(people.iter()) {
        rename_card(&mut stack, *card, person.0);
        add_card_field(
            &mut stack,
            *card,
            "Name",
            Rect::new(90, 24, 300, 22),
            person.0,
        );
        add_card_field(
            &mut stack,
            *card,
            "Phone",
            Rect::new(90, 56, 300, 22),
            person.1,
        );
        add_card_field(
            &mut stack,
            *card,
            "Email",
            Rect::new(90, 88, 300, 22),
            person.2,
        );
        add_card_field(
            &mut stack,
            *card,
            "Notes",
            Rect::new(90, 120, 300, 90),
            person.3,
        );
    }
    stack
}

/// A recipe per card, with a button that does arithmetic on the ingredients:
/// the example that shows chunk expressions earning their keep.
fn recipe_box() -> Stack {
    let mut stack = Stack::new("Recipe Box");
    stack.set_size(Size::new(440, 340));

    let background = stack.backgrounds()[0].id();
    rename_background(&mut stack, background, "Recipe");
    add_label(
        &mut stack,
        background,
        "Recipe Box",
        Rect::new(20, 12, 200, 22),
    );

    add_button(
        &mut stack,
        background,
        "Previous",
        Rect::new(20, 296, 84, 24),
        "on mouseUp\n  go to previous card\nend mouseUp",
    );
    add_button(
        &mut stack,
        background,
        "Next",
        Rect::new(112, 296, 84, 24),
        "on mouseUp\n  go to next card\nend mouseUp",
    );
    add_button(
        &mut stack,
        background,
        "Double It",
        Rect::new(316, 296, 100, 24),
        // Every ingredient is one line with the amount in the first word,
        // which is exactly the shape chunk expressions are for.
        "on mouseUp\n  \
           put empty into doubled\n  \
           repeat with lineNumber = 1 to the number of lines of field \"Ingredients\"\n    \
             put line lineNumber of field \"Ingredients\" into entry\n    \
             put word 1 of entry * 2 into word 1 of entry\n    \
             put entry & return after doubled\n  \
           end repeat\n  \
           put doubled into field \"Ingredients\"\n  \
           answer \"Doubled. There is no undo in the kitchen, but there is one here.\"\n\
         end mouseUp",
    );

    let recipes = [
        (
            "Pancakes",
            "200 g flour\n2 eggs\n300 ml milk\n1 pinch of salt",
            "Whisk everything together and rest it for twenty minutes.\n\
             Fry in a hot pan, one ladle at a time.",
        ),
        (
            "Bread",
            "500 g flour\n7 g yeast\n325 ml water\n10 g salt",
            "Mix, then leave it overnight.\n\
             Shape, prove for two hours, bake hot for forty minutes.",
        ),
    ];

    let mut cards = vec![stack.cards()[0].id()];
    for _ in 1..recipes.len() {
        let card = stack.new_card(background).expect("the background exists");
        let id = card.id();
        stack.add_card(card);
        cards.push(id);
    }

    for (card, recipe) in cards.iter().zip(recipes.iter()) {
        rename_card(&mut stack, *card, recipe.0);
        add_card_field(
            &mut stack,
            *card,
            "Title",
            Rect::new(20, 40, 400, 24),
            recipe.0,
        );
        add_card_field(
            &mut stack,
            *card,
            "Ingredients",
            Rect::new(20, 76, 190, 200),
            recipe.1,
        );
        add_card_field(
            &mut stack,
            *card,
            "Method",
            Rect::new(226, 76, 194, 200),
            recipe.2,
        );
    }
    stack
}

/// One card, one field, three buttons: the smallest thing that is still
/// useful, and the first stack anyone should read.
fn todo() -> Stack {
    let mut stack = Stack::new("Todo");
    stack.set_size(Size::new(380, 300));

    let background = stack.backgrounds()[0].id();
    rename_background(&mut stack, background, "List");
    add_label(
        &mut stack,
        background,
        "Things To Do",
        Rect::new(20, 14, 200, 22),
    );

    let card = stack.cards()[0].id();
    rename_card(&mut stack, card, "Today");
    add_card_field(
        &mut stack,
        card,
        "Items",
        Rect::new(20, 44, 340, 196),
        "x read the HyperTalk reference\nwrite a stack of my own\nshow it to someone",
    );

    add_button(
        &mut stack,
        background,
        "Add",
        Rect::new(20, 256, 80, 24),
        "on mouseUp\n  \
           ask \"What needs doing?\" with \"\"\n  \
           if it is not empty then\n    \
             put it & return after field \"Items\"\n  \
           end if\n\
         end mouseUp",
    );
    add_button(
        &mut stack,
        background,
        "Clear Done",
        Rect::new(108, 256, 100, 24),
        // A line beginning "x " is finished.
        "on mouseUp\n  \
           put empty into remaining\n  \
           repeat with lineNumber = 1 to the number of lines of field \"Items\"\n    \
             put line lineNumber of field \"Items\" into entry\n    \
             if entry is empty then next repeat\n    \
             if entry starts with \"x \" then next repeat\n    \
             put entry & return after remaining\n  \
           end repeat\n  \
           put remaining into field \"Items\"\n\
         end mouseUp",
    );
    add_button(
        &mut stack,
        background,
        "How Many",
        Rect::new(272, 256, 88, 24),
        "on mouseUp\n  \
           put 0 into leftToDo\n  \
           repeat with lineNumber = 1 to the number of lines of field \"Items\"\n    \
             put line lineNumber of field \"Items\" into entry\n    \
             if entry is empty then next repeat\n    \
             if entry starts with \"x \" then next repeat\n    \
             add 1 to leftToDo\n  \
           end repeat\n  \
           answer leftToDo & \" left to do.\"\n\
         end mouseUp",
    );
    stack
}

// ------------------------------------------------------------------ helpers

fn rename_card(stack: &mut Stack, id: Id, name: &str) {
    stack.card_mut(id).expect("the card exists").set_name(name);
}

fn rename_background(stack: &mut Stack, id: Id, name: &str) {
    background_of(stack, id).set_name(name);
}

fn add_button(stack: &mut Stack, background: Id, name: &str, rect: Rect, script: &str) {
    let mut part = stack.new_part(PartKind::Button, name, rect);
    part.set_script(script);
    background_of(stack, background).add_part(part);
}

/// A locked, borderless field used as a caption.
fn add_label(stack: &mut Stack, background: Id, text: &str, rect: Rect) {
    let mut part = stack.new_part(PartKind::Field, format!("{text} Caption"), rect);
    set(&mut part, "text", text);
    set(&mut part, "locked", true);
    set(&mut part, "style", "transparent");
    background_of(stack, background).add_part(part);
}

fn add_card_field(stack: &mut Stack, card: Id, name: &str, rect: Rect, text: &str) {
    let mut part = stack.new_part(PartKind::Field, name, rect);
    set(&mut part, "text", text);
    stack
        .card_mut(card)
        .expect("the card exists")
        .add_part(part);
}

/// A button on one card rather than on the background.
///
/// `bare` makes it invisible artwork-wise: no border, no label, just a
/// clickable region — which is what a room on a drawn board is.
fn add_card_button(stack: &mut Stack, card: Id, name: &str, rect: Rect, script: &str, bare: bool) {
    let mut part = stack.new_part(PartKind::Button, name, rect);
    part.set_script(script);
    if bare {
        set(&mut part, "style", "transparent");
        set(&mut part, "showName", false);
    }
    stack
        .card_mut(card)
        .expect("the card exists")
        .add_part(part);
}

/// A picture on a card, optionally with a script so that clicking it does
/// something.
fn add_card_image(stack: &mut Stack, card: Id, name: &str, rect: Rect, source: &str, script: &str) {
    let mut part = stack.new_part(PartKind::Image, name, rect);
    set(&mut part, "source", source);
    if !script.is_empty() {
        part.set_script(script);
    }
    stack
        .card_mut(card)
        .expect("the card exists")
        .add_part(part);
}

/// A caption on one card.
fn add_card_label(stack: &mut Stack, card: Id, text: &str, rect: Rect) {
    let mut part = stack.new_part(PartKind::Field, format!("{text} Caption"), rect);
    set(&mut part, "text", text);
    set(&mut part, "locked", true);
    set(&mut part, "style", "transparent");
    stack
        .card_mut(card)
        .expect("the card exists")
        .add_part(part);
}

fn background_of(stack: &mut Stack, id: Id) -> &mut Background {
    stack.background_mut(id).expect("the background exists")
}

fn set(part: &mut Part, name: &str, value: impl Into<Value>) {
    part.set_property(name, value.into())
        .expect("these are ordinary properties");
}

// -------------------------------------------------------------- the mansion

/// The nine rooms, and where each sits on `board.svg`.
///
/// The board is one picture and the rooms are transparent buttons laid over
/// it, which is exactly how HyperCard stacks did this: the artwork says where
/// things are and the buttons say what they do.
const ROOMS: [(&str, i32, i32); 9] = [
    ("Kitchen", 0, 0),
    ("Ballroom", 120, 0),
    ("Conservatory", 240, 0),
    ("Dining Room", 0, 100),
    ("Cellar", 120, 100),
    ("Billiard Room", 240, 100),
    ("Lounge", 0, 200),
    ("Hall", 120, 200),
    ("Study", 240, 200),
];

/// Where the board picture sits on the card.
const BOARD: Rect = Rect {
    left: 16,
    top: 40,
    width: 360,
    height: 300,
};

const SUSPECTS: [(&str, &str); 6] = [
    ("Miss Scarlett", "scarlett.svg"),
    ("Colonel Mustard", "mustard.svg"),
    ("Mrs White", "white.svg"),
    ("Mr Green", "green.svg"),
    ("Mrs Peacock", "peacock.svg"),
    ("Professor Plum", "plum.svg"),
];

const WEAPONS: [(&str, &str); 6] = [
    ("Candlestick", "candlestick.svg"),
    ("Dagger", "dagger.svg"),
    ("Lead Pipe", "lead-pipe.svg"),
    ("Revolver", "revolver.svg"),
    ("Rope", "rope.svg"),
    ("Spanner", "spanner.svg"),
];

/// Who did it, with what, and where.
///
/// Fixed rather than drawn at random, for the same reason the timestamps are
/// frozen: an example that plays differently every time cannot be a test.
/// The player still has to deduce it — the field holding it is hidden, and
/// the only way in is to ask.
const SOLUTION: &str = "Professor Plum,Lead Pipe,Conservatory";

/// A game of deduction: the example that shows what pictures are for.
///
/// Every picture is an SVG in `cluedo-art/`, brought in at compile time so
/// that the drawing and the stack cannot drift apart. Suspects and weapons
/// are image parts with scripts — clicking a portrait *is* the choice —
/// while the board is one drawing with transparent buttons over its rooms.
fn cluedo() -> Stack {
    let mut stack = Stack::new("Cluedo");
    stack.set_size(Size::new(640, 460));
    stack.set_script("on openStack\n  go to first card\nend openStack");

    for (name, bytes) in cluedo_art() {
        let image = Image::new(name, bytes.to_vec()).expect("the artwork is checked at build time");
        stack.set_image(name, Some(image));
    }

    let background = stack.backgrounds()[0].id();
    rename_background(&mut stack, background, "Case");
    add_label(
        &mut stack,
        background,
        "A body in the house, and nine rooms to explain it.",
        Rect::new(16, 10, 500, 22),
    );
    for (index, (label, card)) in [
        ("The Mansion", "The Mansion"),
        ("Suspects", "Suspects"),
        ("Weapons", "Weapons"),
    ]
    .iter()
    .enumerate()
    {
        add_button(
            &mut stack,
            background,
            label,
            Rect::new(16 + (index as i32) * 108, 420, 100, 24),
            &format!("on mouseUp\n  go to card \"{card}\"\nend mouseUp"),
        );
    }

    let mansion = stack.cards()[0].id();
    rename_card(&mut stack, mansion, "The Mansion");
    build_mansion(&mut stack, mansion);

    for (name, entries, folder) in [
        ("Suspects", SUSPECTS, "Suspect"),
        ("Weapons", WEAPONS, "Weapon"),
    ] {
        let card = stack.new_card(background).expect("the background exists");
        let id = card.id();
        stack.add_card(card);
        rename_card(&mut stack, id, name);
        build_picker(&mut stack, id, &entries, folder);
    }
    stack
}

/// The artwork, by the name it is known by inside the stack.
fn cluedo_art() -> [(&'static str, &'static [u8]); 13] {
    [
        ("board.svg", include_bytes!("cluedo-art/board.svg")),
        ("scarlett.svg", include_bytes!("cluedo-art/scarlett.svg")),
        ("mustard.svg", include_bytes!("cluedo-art/mustard.svg")),
        ("white.svg", include_bytes!("cluedo-art/white.svg")),
        ("green.svg", include_bytes!("cluedo-art/green.svg")),
        ("peacock.svg", include_bytes!("cluedo-art/peacock.svg")),
        ("plum.svg", include_bytes!("cluedo-art/plum.svg")),
        (
            "candlestick.svg",
            include_bytes!("cluedo-art/candlestick.svg"),
        ),
        ("dagger.svg", include_bytes!("cluedo-art/dagger.svg")),
        ("lead-pipe.svg", include_bytes!("cluedo-art/lead-pipe.svg")),
        ("revolver.svg", include_bytes!("cluedo-art/revolver.svg")),
        ("rope.svg", include_bytes!("cluedo-art/rope.svg")),
        ("spanner.svg", include_bytes!("cluedo-art/spanner.svg")),
    ]
}

fn build_mansion(stack: &mut Stack, card: Id) {
    add_card_image(stack, card, "Board", BOARD, "board.svg", "");

    for (room, left, top) in ROOMS {
        let script = if room == "Cellar" {
            // The one room nothing happened in: it is where the envelope
            // goes, which is worth saying rather than silently ignoring.
            "on mouseUp\n  \
               answer \"The envelope is in the cellar. Nothing happened there.\"\n\
             end mouseUp"
                .to_string()
        } else {
            format!("on mouseUp\n  put \"{room}\" into field \"Room\"\nend mouseUp")
        };
        add_card_button(
            stack,
            card,
            room,
            Rect::new(BOARD.left + left + 3, BOARD.top + top + 3, 114, 94),
            &script,
            true,
        );
    }

    for (index, (caption, field)) in [
        ("Suspect", "Suspect"),
        ("Weapon", "Weapon"),
        ("Room", "Room"),
    ]
    .iter()
    .enumerate()
    {
        let top = 46 + (index as i32) * 54;
        add_card_label(stack, card, caption, Rect::new(392, top, 100, 18));
        let mut part = stack.new_part(PartKind::Field, *field, Rect::new(392, top + 18, 232, 24));
        set(&mut part, "locked", true);
        stack
            .card_mut(card)
            .expect("the card exists")
            .add_part(part);
    }

    add_card_button(
        stack,
        card,
        "Ask",
        Rect::new(392, 214, 110, 26),
        // Three separate comparisons rather than one string equality: the
        // whole game is knowing *how many* of the three were right.
        "on mouseUp\n  \
           if field \"Suspect\" is empty or field \"Weapon\" is empty \
             or field \"Room\" is empty then\n    \
             answer \"Name a suspect, a weapon and a room first.\"\n    \
             exit mouseUp\n  \
           end if\n  \
           put 0 into right\n  \
           if field \"Suspect\" is item 1 of field \"Solution\" then add 1 to right\n  \
           if field \"Weapon\" is item 2 of field \"Solution\" then add 1 to right\n  \
           if field \"Room\" is item 3 of field \"Solution\" then add 1 to right\n  \
           put field \"Suspect\" & \", \" & field \"Weapon\" & \", \" & field \"Room\" \
             & \" — \" & right & \" of 3\" & return before field \"Replies\"\n\
         end mouseUp",
        false,
    );
    add_card_button(
        stack,
        card,
        "Accuse",
        Rect::new(514, 214, 110, 26),
        "on mouseUp\n  \
           if field \"Suspect\" is item 1 of field \"Solution\" \
             and field \"Weapon\" is item 2 of field \"Solution\" \
             and field \"Room\" is item 3 of field \"Solution\" then\n    \
             answer \"Case closed. \" & item 1 of field \"Solution\" & \", in the \" \
               & item 3 of field \"Solution\" & \", with the \" \
               & item 2 of field \"Solution\" & \".\"\n  \
           else\n    \
             answer \"No. Somebody is still at large.\"\n  \
           end if\n\
         end mouseUp",
        false,
    );

    add_card_label(stack, card, "Replies", Rect::new(392, 250, 100, 18));
    let mut replies = stack.new_part(PartKind::Field, "Replies", Rect::new(392, 268, 232, 100));
    set(&mut replies, "locked", true);
    stack
        .card_mut(card)
        .expect("the card exists")
        .add_part(replies);

    add_card_button(
        stack,
        card,
        "Start Over",
        Rect::new(392, 378, 232, 26),
        "on mouseUp\n  \
           put empty into field \"Suspect\"\n  \
           put empty into field \"Weapon\"\n  \
           put empty into field \"Room\"\n  \
           put empty into field \"Replies\"\n\
         end mouseUp",
        false,
    );

    // Hidden rather than absent: the answer has to live somewhere, and a
    // field is the only container a stack owns. Anyone can find it in the
    // bundle, which is the honest cost of a stack being readable.
    let mut solution = stack.new_part(PartKind::Field, "Solution", Rect::new(392, 410, 232, 22));
    set(&mut solution, "text", SOLUTION);
    set(&mut solution, "visible", false);
    set(&mut solution, "locked", true);
    stack
        .card_mut(card)
        .expect("the card exists")
        .add_part(solution);
}

/// A card of pictures you choose from by clicking one.
fn build_picker(stack: &mut Stack, card: Id, entries: &[(&str, &str); 6], field: &str) {
    add_card_label(
        stack,
        card,
        &format!("Click a {} to name it.", field.to_lowercase()),
        Rect::new(24, 44, 400, 20),
    );

    for (index, (name, picture)) in entries.iter().enumerate() {
        let left = 40 + (index as i32 % 3) * 190;
        let top = 82 + (index as i32 / 3) * 160;
        add_card_image(
            stack,
            card,
            name,
            Rect::new(left + 33, top, 96, 96),
            picture,
            // The choice is the picture, which is the point of a picture
            // being a part: no invisible button laid over the top.
            &format!(
                "on mouseUp\n  \
                   put \"{name}\" into field \"{field}\" of card \"The Mansion\"\n  \
                   go to card \"The Mansion\"\n\
                 end mouseUp"
            ),
        );
        add_card_label(stack, card, name, Rect::new(left, top + 102, 162, 20));
    }
}
