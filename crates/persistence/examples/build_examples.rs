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

use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};

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
        ("Myst", myst()),
        ("Language Models, Explained", deck()),
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
/// constant, so regenerating one changes nothing unless the program did —
/// which is what lets CI check that `examples/` still matches this file. The
/// bundle's own `savedAt` is the exception, and CI excludes it.
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
        // The trailing newline matters: the Add button does `put it & return
        // after field "Items"`, and without it the first thing anyone adds
        // glues itself to "show it to someone".
        "x read the HyperTalk reference\nwrite a stack of my own\nshow it to someone\n",
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

/// A caption on one card, named after what it says.
fn add_card_label(stack: &mut Stack, card: Id, text: &str, rect: Rect) {
    add_card_caption(stack, card, &format!("{text} Caption"), text, rect);
}

/// A slide's heading: the same text a caption would hold, in a box.
///
/// HyperLab has no `textSize`, so a title cannot be set in larger type. A
/// shadowed box is what HyperCard would have done anyway, and it separates
/// the heading from the paragraph under it, which is the whole job.
fn add_card_title(stack: &mut Stack, card: Id, text: &str, rect: Rect) {
    let mut part = stack.new_part(PartKind::Field, "Title", rect);
    set(&mut part, "text", text);
    set(&mut part, "locked", true);
    set(&mut part, "style", "shadow");
    stack
        .card_mut(card)
        .expect("the card exists")
        .add_part(part);
}

/// A caption on one card with a name of its own, for text too long to be one.
fn add_card_caption(stack: &mut Stack, card: Id, name: &str, text: &str, rect: Rect) {
    let mut part = stack.new_part(PartKind::Field, name, rect);
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

/// A room on the board: its name, and the rectangle it occupies.
struct Room {
    name: &'static str,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

/// The nine rooms and the cellar, laid out like a house rather than a grid.
///
/// One table, used twice: [`board_svg`] draws from it and the buttons are
/// placed from it. The board is a picture with transparent buttons over its
/// rooms, which is how a HyperCard stack did this — and the reason the
/// picture is generated rather than drawn by hand is that a hand-drawn one
/// drifts away from the buttons the first time a wall moves.
const ROOMS: [Room; 10] = [
    Room {
        name: "Kitchen",
        left: 8,
        top: 8,
        width: 96,
        height: 80,
    },
    Room {
        name: "Ballroom",
        left: 116,
        top: 8,
        width: 128,
        height: 80,
    },
    Room {
        name: "Conservatory",
        left: 256,
        top: 8,
        width: 96,
        height: 80,
    },
    Room {
        name: "Dining Room",
        left: 8,
        top: 100,
        width: 96,
        height: 92,
    },
    Room {
        name: "Cellar",
        left: 124,
        top: 100,
        width: 112,
        height: 92,
    },
    Room {
        name: "Billiard Room",
        left: 256,
        top: 100,
        width: 96,
        height: 54,
    },
    Room {
        name: "Library",
        left: 256,
        top: 166,
        width: 96,
        height: 42,
    },
    Room {
        name: "Lounge",
        left: 8,
        top: 204,
        width: 96,
        height: 88,
    },
    Room {
        name: "Hall",
        left: 124,
        top: 204,
        width: 112,
        height: 88,
    },
    Room {
        name: "Study",
        left: 256,
        top: 220,
        width: 96,
        height: 72,
    },
];

/// Where the board picture sits on the card.
const BOARD: Rect = Rect {
    left: 16,
    top: 40,
    width: 360,
    height: 300,
};

/// The floor plan, drawn from [`ROOMS`].
///
/// One bit deep, the way HyperCard was: there are no greys here, only ink,
/// paper, and a dither where a tone is wanted. The hallways are a 50%
/// checkerboard, the rooms are paper, every wall is solid, and the cellar is
/// solid black with the envelope showing white inside it.
fn board_svg() -> String {
    let mut out = String::from(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 360 300\" \
         width=\"360\" height=\"300\">\n\
         \x20 <title>A mansion, nine rooms and a cellar</title>\n\
         \x20 <defs>\n\
         \x20   <pattern id=\"hall\" width=\"4\" height=\"4\" patternUnits=\"userSpaceOnUse\">\n\
         \x20     <rect width=\"4\" height=\"4\" fill=\"#fff\"/>\n\
         \x20     <rect width=\"2\" height=\"2\"/><rect x=\"2\" y=\"2\" width=\"2\" height=\"2\"/>\n\
         \x20   </pattern>\n\
         \x20 </defs>\n\
         \x20 <rect width=\"360\" height=\"300\" fill=\"url(#hall)\"/>\n",
    );

    for room in &ROOMS {
        let cellar = room.name == "Cellar";
        let (fill, ink) = if cellar {
            ("#000", "#fff")
        } else {
            ("#fff", "#000")
        };
        let (right, bottom) = (room.left + room.width, room.top + room.height);
        let _ = writeln!(
            out,
            "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{fill}\" \
             stroke=\"#000\" stroke-width=\"4\"/>",
            room.left, room.top, room.width, room.height
        );

        // A doorway: a gap knocked in the wall nearest the hallway.
        if !cellar {
            let middle = room.left + room.width / 2;
            let door = if bottom < 150 {
                format!("M{} {bottom} h24", middle - 12)
            } else if room.top > 150 {
                format!("M{} {} h24", middle - 12, room.top)
            } else if room.left < 120 {
                format!("M{right} {} v24", room.top + room.height / 2 - 12)
            } else {
                format!("M{} {} v24", room.left, room.top + room.height / 2 - 12)
            };
            let _ = writeln!(
                out,
                "  <path d=\"{door}\" stroke=\"#fff\" stroke-width=\"5\"/>"
            );
        }

        let middle_x = room.left + room.width / 2;
        let middle_y = room.top + room.height / 2;
        let words: Vec<&str> = room.name.split(' ').collect();
        let mut label = |text: &str, y: i32| {
            let _ = writeln!(
                out,
                "  <text x=\"{middle_x}\" y=\"{y}\" fill=\"{ink}\" \
                 font-family=\"Chicago,ChicagoFLF,Geneva,Verdana,sans-serif\" \
                 font-size=\"12\" font-weight=\"bold\" text-anchor=\"middle\">{text}</text>"
            );
        };
        if let [first, second] = words[..] {
            label(first, middle_y - 1);
            label(second, middle_y + 13);
        } else {
            label(room.name, middle_y + 4);
        }

        if cellar {
            // The envelope, face down in the middle of the house.
            let (x, y) = (middle_x - 19, room.top + 12);
            let _ = writeln!(
                out,
                "  <rect x=\"{x}\" y=\"{y}\" width=\"38\" height=\"24\" fill=\"#fff\" \
                 stroke=\"#000\" stroke-width=\"2\"/>\n\
                 \x20 <path d=\"M{x} {y} l19 14 l19 -14\" fill=\"none\" stroke=\"#000\" \
                 stroke-width=\"2\"/>"
            );
        }
    }

    out.push_str(
        "  <rect x=\"2\" y=\"2\" width=\"356\" height=\"296\" fill=\"none\" \
         stroke=\"#000\" stroke-width=\"4\"/>\n</svg>\n",
    );
    out
}

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

    let board = board_svg();
    stack.set_image(
        "board.svg",
        Some(Image::new("board.svg", board.into_bytes()).expect("the floor plan is a valid SVG")),
    );
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

/// The hand-drawn artwork, by the name it is known by inside the stack.
///
/// The board is not here: it is generated from [`ROOMS`] so that the picture
/// and the buttons over it cannot disagree.
fn cluedo_art() -> [(&'static str, &'static [u8]); 12] {
    [
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

    for room in &ROOMS {
        let script = if room.name == "Cellar" {
            // The one room nothing happened in: it is where the envelope
            // goes, which is worth saying rather than silently ignoring.
            "on mouseUp\n  \
               answer \"The envelope is in the cellar. Nothing happened there.\"\n\
             end mouseUp"
                .to_string()
        } else {
            format!(
                "on mouseUp\n  put \"{}\" into field \"Room\"\nend mouseUp",
                room.name
            )
        };
        add_card_button(
            stack,
            card,
            room.name,
            Rect::new(
                BOARD.left + room.left,
                BOARD.top + room.top,
                room.width,
                room.height,
            ),
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

// ------------------------------------------------------------------- an island

/// One place, its picture, what it says, and the ways out of it.
struct Place {
    name: &'static str,
    picture: &'static str,
    blurb: &'static str,
    exits: &'static [(&'static str, &'static str)],
}

/// An island, four Ages, and one place with no way back.
///
/// Not a port of anything — an original stack shaped like the thing
/// [`crates/graph`](../graph) was built after. Myst is a hub with spokes:
/// almost everything runs through the library, and the interesting question
/// about it is structural rather than visual, which is why two separate
/// projects have gone to the trouble of extracting its graph.
const PLACES: [Place; 11] = [
    Place {
        name: "Dock",
        picture: "dock.svg",
        blurb: "You arrive here and the book closes behind you. \
                A path climbs away from the water.",
        exits: &[("Up to the library", "Library")],
    },
    Place {
        name: "Library",
        picture: "library.svg",
        blurb: "Two shelves burned. The four books that did not are each a \
                place, and each of them is somewhere you can go.",
        exits: &[
            ("Down to the dock", "Dock"),
            ("The clock tower", "Clock Tower"),
            ("The planetarium", "Planetarium"),
            ("The generator room", "Generator Room"),
            ("The linking books", "Linking Books"),
        ],
    },
    Place {
        name: "Clock Tower",
        picture: "clock-tower.svg",
        blurb: "The hands answer to two brass wheels on the shore. \
                Somebody has left them at two and two.",
        exits: &[("Back to the library", "Library")],
    },
    Place {
        name: "Planetarium",
        picture: "planetarium.svg",
        blurb: "A dome, a dial, and a sky that will show you any date you \
                ask it for.",
        exits: &[("Back to the library", "Library")],
    },
    Place {
        name: "Generator Room",
        picture: "generator.svg",
        blurb: "Dials, breakers, and a needle that has to sit between two \
                marks before anything else on this island works.",
        exits: &[("Back to the library", "Library")],
    },
    Place {
        name: "Linking Books",
        picture: "linking-books.svg",
        blurb: "Four lecterns. Put your hand on the page and you are \
                somewhere else before you have finished deciding to.",
        exits: &[
            ("Back to the library", "Library"),
            ("Channelwood", "Channelwood"),
            ("Mechanical", "Mechanical Age"),
            ("Selenitic", "Selenitic Age"),
            ("Stoneship", "Stoneship Age"),
        ],
    },
    Place {
        name: "Channelwood",
        picture: "channelwood.svg",
        blurb: "Walkways lashed between trees, over water that goes down \
                further than the light does.",
        exits: &[("Link home", "Linking Books")],
    },
    Place {
        name: "Mechanical Age",
        picture: "mechanical.svg",
        blurb: "A fortress on a pivot. Turn the handle and the whole \
                building faces somewhere new.",
        exits: &[("Link home", "Linking Books")],
    },
    Place {
        name: "Selenitic Age",
        picture: "selenitic.svg",
        blurb: "Craters, and five sounds carried on aerials to a room that \
                is listening for them in the right order.",
        exits: &[("Link home", "Linking Books")],
    },
    Place {
        name: "Stoneship Age",
        picture: "stoneship.svg",
        blurb: "A ship in the rock, and a compass rose with a book set into \
                it. The book is not labelled.",
        exits: &[
            ("Link home", "Linking Books"),
            ("Open the unlabelled book", "D'ni"),
        ],
    },
    Place {
        // The point of the example, and of the map: a card you can reach and
        // cannot leave. A trap book is exactly the shape of that bug.
        name: "D'ni",
        picture: "dni.svg",
        blurb: "Rock, and a long way down, a light.\n\nThe book you came \
                through is not here, and neither is any other.",
        exits: &[],
    },
];

/// An island you move around by clicking, and a graph worth looking at.
fn myst() -> Stack {
    let mut stack = Stack::new("Myst");
    stack.set_size(Size::new(600, 400));
    stack.set_script("on openStack\n  go to first card\nend openStack");

    for (name, bytes) in myst_art() {
        let image = Image::new(name, bytes.to_vec()).expect("the artwork is checked at build time");
        stack.set_image(name, Some(image));
    }

    let background = stack.backgrounds()[0].id();
    rename_background(&mut stack, background, "Age");

    let first = stack.cards()[0].id();
    let mut cards = vec![first];
    for _ in 1..PLACES.len() {
        let card = stack.new_card(background).expect("the background exists");
        let id = card.id();
        stack.add_card(card);
        cards.push(id);
    }

    for (card, place) in cards.iter().zip(PLACES.iter()) {
        rename_card(&mut stack, *card, place.name);
        add_card_image(
            &mut stack,
            *card,
            place.name,
            Rect::new(16, 16, 320, 200),
            place.picture,
            "",
        );
        add_card_field(
            &mut stack,
            *card,
            "Place",
            Rect::new(352, 16, 232, 24),
            place.name,
        );
        add_card_field(
            &mut stack,
            *card,
            "About",
            Rect::new(352, 48, 232, 100),
            place.blurb,
        );
        for (index, (label, destination)) in place.exits.iter().enumerate() {
            add_card_button(
                &mut stack,
                *card,
                label,
                Rect::new(352, 162 + (index as i32) * 32, 232, 26),
                &format!("on mouseUp\n  go to card \"{destination}\"\nend mouseUp"),
                false,
            );
        }
    }
    stack
}

/// The scenery, by the name it is known by inside the stack.
fn myst_art() -> [(&'static str, &'static [u8]); 11] {
    [
        ("dock.svg", include_bytes!("myst-art/dock.svg")),
        ("library.svg", include_bytes!("myst-art/library.svg")),
        (
            "clock-tower.svg",
            include_bytes!("myst-art/clock-tower.svg"),
        ),
        (
            "planetarium.svg",
            include_bytes!("myst-art/planetarium.svg"),
        ),
        ("generator.svg", include_bytes!("myst-art/generator.svg")),
        (
            "linking-books.svg",
            include_bytes!("myst-art/linking-books.svg"),
        ),
        (
            "channelwood.svg",
            include_bytes!("myst-art/channelwood.svg"),
        ),
        ("mechanical.svg", include_bytes!("myst-art/mechanical.svg")),
        ("selenitic.svg", include_bytes!("myst-art/selenitic.svg")),
        ("stoneship.svg", include_bytes!("myst-art/stoneship.svg")),
        ("dni.svg", include_bytes!("myst-art/dni.svg")),
    ]
}

// --------------------------------------------------------------- the deck

/// What sits in the middle of a slide, under the paragraph.
enum Illustration {
    /// A diagram, by the name the stack knows the picture by.
    Diagram(&'static str),
    /// A question box, a button, and room for a live model's answer.
    Ask,
    /// The reference list the citation lines have been building towards.
    Bibliography,
}

/// One slide: its heading, the paragraph, the source to go to next, and
/// whatever is drawn between them.
///
/// `reading` is the citation shown while the idea is still on screen, which
/// is where a citation is worth anything. The last card collects them and
/// adds the books, the explainers and the code.
struct Slide {
    title: &'static str,
    body: &'static str,
    reading: &'static str,
    under: Illustration,
}

/// The deck, in the order it is shown.
const SLIDES: [Slide; 10] = [
    Slide {
        title: "Language Models, Explained",
        body: "What follows is a working picture of how a language model turns text into \
               more text. It is coarse in places and nowhere false, and it is enough to \
               reason about the thing rather than guess at it.\n\n\
               Each card cites where to read further. The last one asks a live model, if \
               you have configured one. Press Next.",
        reading: "Jurafsky & Martin, Speech and Language Processing, 3rd ed. draft \u{b7} \
                  web.stanford.edu/~jurafsky/slp3",
        under: Illustration::Diagram("pipeline.svg"),
    },
    Slide {
        title: "It predicts the next token",
        body: "The model reads a sequence of tokens and scores every token in its \
               vocabulary \u{2014} tens of thousands of numbers, one per candidate. A \
               softmax turns those scores into a probability distribution, one token is \
               drawn from it and appended, and the whole sequence goes through \
               again.\n\n\
               That loop is the entire mechanic. Everything after this card follows from \
               it.",
        reading: "Vaswani et al., Attention Is All You Need, NeurIPS 2017 \u{b7} \
                  arXiv:1706.03762\n\
                  Alammar, The Illustrated Transformer \u{b7} \
                  jalammar.github.io/illustrated-transformer",
        under: Illustration::Diagram("next-token.svg"),
    },
    Slide {
        title: "A token is not a word",
        body: "A tokenizer splits text into subword units learned by byte-pair encoding, \
               which merges the commonest adjacent pairs over and over. Frequent words \
               survive whole while rare ones shatter, and a leading space usually \
               belongs to the token after it.\n\n\
               The model never sees letters, so counting the r\u{2019}s in a word asks it \
               about something it was never given.",
        reading: "Sennrich, Haddow & Birch, Neural Machine Translation of Rare Words with \
                  Subword Units, ACL 2016 \u{b7} arXiv:1508.07909\n\
                  Watch a tokenizer work: tiktokenizer.vercel.app",
        under: Illustration::Diagram("tokens.svg"),
    },
    Slide {
        title: "The context window is all it knows",
        body: "Nothing persists between calls. The weights are frozen, so whatever a chat \
               appears to remember about you was re-sent as text in that same request, \
               and the window holding it has a hard limit in tokens.\n\n\
               Position inside that window matters: models use what sits at the start and \
               the end of a long context far more reliably than what is buried in the \
               middle.",
        reading: "Liu et al., Lost in the Middle: How Language Models Use Long Contexts, \
                  TACL 2024 \u{b7} arXiv:2307.03172",
        under: Illustration::Diagram("context.svg"),
    },
    Slide {
        title: "Temperature reshapes the distribution",
        body: "Sampling is a stage after the model, and the scores it is handed do not \
               change. Temperature divides them before the softmax: under one sharpens \
               the distribution toward the top candidates, over one flattens it. Top-p \
               keeps the smallest set of tokens whose probability sums to p.\n\n\
               Turning it down buys consistency. Truthfulness is not on this dial.",
        reading: "Holtzman et al., The Curious Case of Neural Text Degeneration, ICLR 2020 \
                  \u{b7} arXiv:1904.09751",
        under: Illustration::Diagram("temperature.svg"),
    },
    Slide {
        title: "It is not looking anything up",
        body: "There is no index and no retrieval step. An answer is computed in one \
               forward pass from billions of weights that encode statistical regularities \
               of the training data, so facts seen often come back well and facts seen \
               rarely are reconstructed \u{2014} with the same fluent cadence either \
               way.\n\n\
               If it has to be right, put the source in the context and say so.",
        reading: "Lewis et al., Retrieval-Augmented Generation for Knowledge-Intensive NLP \
                  Tasks, NeurIPS 2020 \u{b7} arXiv:2005.11401",
        under: Illustration::Diagram("weights.svg"),
    },
    Slide {
        title: "A prompt is just more context",
        body: "Nothing in a prompt is a command. Your text conditions the same next-token \
               distribution everything else conditions, which is why an incantation like \
               \u{201c}be accurate\u{201d} moves almost nothing while one worked example \
               moves a great deal: the behaviour was already there, and the example \
               locates it.\n\n\
               Show the shape of the answer you want, then hand over the material.",
        reading: "Brown et al., Language Models are Few-Shot Learners, NeurIPS 2020 \u{b7} \
                  arXiv:2005.14165",
        under: Illustration::Diagram("prompt.svg"),
    },
    Slide {
        title: "Tools: it asks, something else acts",
        body: "A model emits tokens and nothing else. Given a schema of the calls \
               available to it, it can emit one \u{2014} a name and arguments \u{2014} and \
               stop. Your program parses that, does the work, and feeds the result back \
               as more tokens.\n\n\
               HyperLab's assistant works this way, and each action it takes is a \
               Command, which is why you can undo it.",
        reading: "Yao et al., ReAct: Synergizing Reasoning and Acting in Language Models, \
                  ICLR 2023 \u{b7} arXiv:2210.03629",
        under: Illustration::Diagram("tools.svg"),
    },
    Slide {
        title: "Now ask a real one",
        body: "Everything so far was written in advance. What comes back below will not \
               be. Configure a provider under AI \u{25b8} Show Assistant \u{25b8} Settings, \
               then press Ask.\n\n\
               With none configured it says so and the stack keeps working, which is a \
               rule the runtime enforces.",
        reading: "Ouyang et al., Training language models to follow instructions with \
                  human feedback, NeurIPS 2022 \u{b7} arXiv:2203.02155",
        under: Illustration::Ask,
    },
    Slide {
        title: "Where to read next",
        body: "Everything cited on the way here, and a few places to go further than these \
               cards can.",
        reading: "",
        under: Illustration::Bibliography,
    },
];

/// The reference list on the last card.
///
/// Written out rather than gathered from the `reading` lines above, because
/// half of it never appears on a slide: the textbook, the explainers, the
/// code you can read in an afternoon.
const BIBLIOGRAPHY: &str = "\
Jurafsky & Martin, Speech and Language Processing, 3rd ed. draft \u{b7} \
web.stanford.edu/~jurafsky/slp3
Alammar, The Illustrated Transformer \u{b7} jalammar.github.io/illustrated-transformer
Karpathy, nanoGPT \u{2014} a GPT in about 300 lines \u{b7} github.com/karpathy/nanoGPT
Elhage et al., A Mathematical Framework for Transformer Circuits, 2021 \u{b7} \
transformer-circuits.pub
Bender, Gebru, McMillan-Major & Shmitchell, On the Dangers of Stochastic Parrots, \
FAccT 2021
Wei et al., Chain-of-Thought Prompting Elicits Reasoning in Large Language Models, \
NeurIPS 2022 \u{b7} arXiv:2201.11903
Ji et al., Survey of Hallucination in Natural Language Generation, ACM Computing \
Surveys \u{b7} arXiv:2202.03629
Schick et al., Toolformer: Language Models Can Teach Themselves to Use Tools, 2023 \
\u{b7} arXiv:2302.04761";

/// A slide deck about language models, driven by one on the last card.
fn deck() -> Stack {
    let mut stack = Stack::new("Language Models, Explained");
    stack.set_size(Size::new(640, 500));
    stack.set_script("on openStack\n  go to first card\nend openStack");

    for (name, bytes) in deck_art() {
        let image = Image::new(name, bytes.to_vec()).expect("the artwork is checked at build time");
        stack.set_image(name, Some(image));
    }

    let background = stack.backgrounds()[0].id();
    rename_background(&mut stack, background, "Slide");

    let first = stack.cards()[0].id();
    let mut cards = vec![first];
    for _ in 1..SLIDES.len() {
        let card = stack.new_card(background).expect("the background exists");
        let id = card.id();
        stack.add_card(card);
        cards.push(id);
    }

    for (index, (card, slide)) in cards.iter().zip(SLIDES.iter()).enumerate() {
        rename_card(&mut stack, *card, slide.title);
        add_card_title(&mut stack, *card, slide.title, Rect::new(24, 12, 592, 30));

        // The reference list needs the height a diagram would have taken, so
        // its paragraph is the one short one in the deck.
        let body = match slide.under {
            Illustration::Bibliography => Rect::new(24, 50, 592, 60),
            _ => Rect::new(24, 50, 592, 112),
        };
        add_card_caption(&mut stack, *card, "Body", slide.body, body);

        match slide.under {
            Illustration::Diagram(picture) => add_card_image(
                &mut stack,
                *card,
                slide.title,
                Rect::new(24, 170, 592, 200),
                picture,
                "",
            ),
            Illustration::Ask => build_ask_slide(&mut stack, *card),
            Illustration::Bibliography => add_card_caption(
                &mut stack,
                *card,
                "References",
                BIBLIOGRAPHY,
                Rect::new(24, 118, 592, 312),
            ),
        }

        if !slide.reading.is_empty() {
            add_card_caption(
                &mut stack,
                *card,
                "Reading",
                slide.reading,
                Rect::new(24, 376, 592, 54),
            );
        }

        // Each card says where it is. The total is counted rather than
        // written down, so adding a slide cannot make the deck lie.
        stack
            .card_mut(*card)
            .expect("the card exists")
            .set_script(&format!(
                "on openCard\n  \
                   put \"{} of \" & the number of cards into field \"Where\"\n\
                 end openCard",
                index + 1
            ));
    }

    add_button(
        &mut stack,
        background,
        "Back",
        Rect::new(24, 442, 84, 26),
        "on mouseUp\n  go to previous card\nend mouseUp",
    );
    add_button(
        &mut stack,
        background,
        "Next",
        Rect::new(116, 442, 84, 26),
        "on mouseUp\n  go to next card\nend mouseUp",
    );
    add_button(
        &mut stack,
        background,
        "Start Over",
        Rect::new(208, 442, 100, 26),
        "on mouseUp\n  go to first card\nend mouseUp",
    );

    let mut where_are_we = stack.new_part(PartKind::Field, "Where", Rect::new(500, 444, 116, 22));
    set(&mut where_are_we, "locked", true);
    set(&mut where_are_we, "style", "transparent");
    background_of(&mut stack, background).add_part(where_are_we);

    stack
}

/// The last slide: a question, a button, and room for whatever comes back.
///
/// `ask assistant` rather than `ai(…)` on purpose. A refused `ai(…)` stops the
/// handler; a refused `ask assistant` leaves the reason in `the result`, so
/// the slide explains itself on a machine with no model set up instead of
/// reporting a script error.
fn build_ask_slide(stack: &mut Stack, card: Id) {
    add_card_field(
        stack,
        card,
        "Question",
        Rect::new(24, 170, 592, 44),
        "Explain a context window to someone who has never heard of one.",
    );
    add_card_button(
        stack,
        card,
        "Ask",
        Rect::new(24, 222, 96, 26),
        "on mouseUp\n  \
           put field \"Question\" into asked\n  \
           if asked is empty then\n    \
             answer \"Type a question first.\"\n    \
             exit mouseUp\n  \
           end if\n  \
           ask assistant asked\n  \
           if the result is not empty then\n    \
             put the result into field \"Answer\"\n  \
           else\n    \
             put it into field \"Answer\"\n  \
           end if\n\
         end mouseUp",
        false,
    );
    add_card_field(stack, card, "Answer", Rect::new(24, 256, 592, 114), "");
}

/// The diagrams, by the name they are known by inside the stack.
fn deck_art() -> [(&'static str, &'static [u8]); 8] {
    [
        ("pipeline.svg", include_bytes!("deck-art/pipeline.svg")),
        ("next-token.svg", include_bytes!("deck-art/next-token.svg")),
        ("tokens.svg", include_bytes!("deck-art/tokens.svg")),
        ("context.svg", include_bytes!("deck-art/context.svg")),
        (
            "temperature.svg",
            include_bytes!("deck-art/temperature.svg"),
        ),
        ("weights.svg", include_bytes!("deck-art/weights.svg")),
        ("prompt.svg", include_bytes!("deck-art/prompt.svg")),
        ("tools.svg", include_bytes!("deck-art/tools.svg")),
    ]
}
