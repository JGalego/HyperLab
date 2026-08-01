//! Every example, translated.
//!
//! What these cannot check is whether the result runs: that needs a browser
//! and the real library, and it was checked in one. What they hold on to is
//! the ground that check won — that the six stacks in `examples/` come across,
//! and that the constructs found to be parse errors are never emitted again.

use std::path::{Path, PathBuf};

use hyperlab_hyperscript::page;
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
    "LLMs for n00bs",
];

#[test]
fn every_example_becomes_a_page() {
    for name in EVERY {
        let written = page(&open(name)).source;
        assert!(written.starts_with("<!doctype html>"), "{name}");
        assert!(written.contains("hyperscript.org@0.9.93"), "{name}");
        assert!(written.ends_with("</html>\n"), "{name}: truncated");
    }
}

#[test]
fn only_the_assistant_fails_to_come_across() {
    // Five of the six translate whole. The sixth asks a language model on its
    // last card, and a page has none — so exactly one note, and it says that.
    for name in EVERY {
        let notes = page(&open(name)).notes;
        if name == "LLMs for n00bs" {
            assert_eq!(notes, vec!["a page has no assistant to ask".to_string()]);
        } else {
            assert!(notes.is_empty(), "{name} left something behind: {notes:?}");
        }
    }
}

#[test]
fn nothing_a_browser_refuses_to_parse_is_emitted() {
    // Each of these was a parse error found by loading a page in Chromium with
    // the real library, and each failed quietly: _hyperscript logs its
    // complaint to the console and leaves the handler dead.
    for name in EVERY {
        let written = page(&open(name)).source;
        assert!(
            !written.contains(") times"),
            "{name}: `repeat (…) times` does not parse; the count must be a name"
        );
        assert!(
            !written.contains("split(/"),
            "{name}: a regular expression literal does not parse in a handler"
        );
        assert!(
            !written.contains("set it to"),
            "{name}: `it` cannot be assigned; it silently stays null"
        );
        assert!(
            !written.contains("call ask assistant("),
            "{name}: `ask assistant` is two words, not a function"
        );
    }
}

#[test]
fn a_cross_card_reference_finds_the_card_it_names() {
    // Cluedo's pickers write into a field on another card by naming it. All
    // twelve have to resolve, or choosing a suspect does nothing at all.
    let written = page(&open("Cluedo")).source;
    let suspect = written
        .matches("hl-card-1-card-suspect&#39;s value")
        .count();
    let weapon = written.matches("hl-card-1-card-weapon&#39;s value").count();
    assert!(
        suspect >= 6,
        "six suspects write to the mansion, saw {suspect}"
    );
    assert!(
        weapon >= 6,
        "six weapons write to the mansion, saw {weapon}"
    );
}

#[test]
fn the_glue_defines_everything_the_scripts_call() {
    // A translated script calls into the page's own helpers, so a missing one
    // is a handler that throws on the first click.
    let written = page(&open("Recipe Box")).source;
    for helper in ["hlGo", "hlGoTo", "hlBack", "hlSplice", "hlPart", "hlCount"] {
        assert!(
            written.contains(&format!("function {helper}(")),
            "the page never defines {helper}"
        );
    }
    // And the one thing a script reads rather than calls has to be reachable:
    // `const` at the top of a script is not on the global object.
    assert!(written.contains("window.hlCards = hlCards"));
}
