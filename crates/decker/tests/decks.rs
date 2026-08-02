//! Every example, as a deck.
//!
//! What these cannot check is whether Decker runs the result: that needs the
//! Decker runtime, and each of the six was opened in it and played. What they
//! hold on to is the ground that won — that the stacks come across, and that
//! the Lil found to be wrong there is never written again.

use std::path::{Path, PathBuf};

use hyperlab_decker::deck;
use hyperlab_persistence::load;
use hyperlab_stack::Stack;

fn open(name: &str) -> Stack {
    let examples: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate is two levels below the repository root")
        .join("examples");
    load(examples.join(format!("{name}.hl"))).unwrap_or_else(|error| panic!("{name}: {error}"))
}

const EVERY: [&str; 6] = [
    "Address Book",
    "Recipe Box",
    "Todo",
    "Cluedo",
    "Myst",
    "Language Models, Explained",
];

#[test]
fn every_example_becomes_a_deck() {
    for name in EVERY {
        let written = deck(&open(name)).source;
        assert!(written.contains("\n{deck}\nversion:1\n"), "{name}");
        assert!(written.contains("\n{widgets}\n"), "{name}");
        assert!(
            written.trim_end().ends_with("</script>"),
            "{name}: truncated"
        );
    }
}

#[test]
fn only_the_assistant_and_the_opening_fail_to_come_across() {
    // Four translate whole. Three ask their stack to go to the first card as
    // it opens, which a deck does by itself; the sixth asks a language model,
    // and a deck has none.
    for name in EVERY {
        let notes = deck(&open(name)).notes;
        let opening = "a deck has no moment of opening to run a script at".to_string();
        let expected: Vec<String> = match name {
            "Address Book" | "Myst" | "Cluedo" => vec![opening],
            "Language Models, Explained" => vec![
                "\"ask assistant\" is not translated".to_string(),
                "\"the result\" is not translated".to_string(),
                "\"the result\" is not translated".to_string(),
                opening,
            ],
            _ => Vec::new(),
        };
        assert_eq!(notes, expected, "{name}");
    }
}

#[test]
fn nothing_lil_refuses_is_written() {
    // Each of these was found by opening a deck in Decker and watching a
    // handler do the wrong thing quietly.
    for name in EVERY {
        let written = deck(&open(name)).source;
        assert!(
            !written.contains("<=") && !written.contains(">="),
            "{name}: Lil has `<` and `>` and nothing that takes the ends in"
        );
        assert!(
            !written.contains("(~("),
            "{name}: `~` is match and takes two operands; `!` is not"
        );
        // A slash begins a comment, so one left bare in base64 artwork cuts
        // the rest of the line — and the picture — away without a word.
        for line in written.lines() {
            let Some(record) = line.strip_prefix("image:\"") else {
                continue;
            };
            for (at, _) in record.match_indices('/') {
                assert_eq!(&record[at - 1..at], "\\", "{name}: a bare slash in artwork");
            }
        }
    }
}

#[test]
fn a_card_that_draws_something_carries_a_bitmap_the_size_of_the_deck() {
    // Decker shows nothing at all for an image record whose size is not the
    // deck's, which is how the rule was found.
    let written = deck(&open("Myst")).source;
    let size = written
        .lines()
        .find_map(|line| line.strip_prefix("size:"))
        .expect("a deck says how big it is");
    assert_eq!(size, "[600,400]");

    let record = written
        .lines()
        .find_map(|line| line.strip_prefix("image:\""))
        .expect("the island is drawn");
    // Four bytes of size, base64 six bits at a time: `AlgBkA` is 600 × 400.
    assert!(record.starts_with("%%IMG0AlgBkA"), "{record:.32}");
}
