//! Every example, exported.
//!
//! A PDF that does not open is worse than no export, and the only way to know
//! is to build one from a real stack rather than a fixture. These read the
//! bundles in `examples/`, which are the same ones the runtime tests click
//! through.

use std::path::{Path, PathBuf};

use hyperlab_export::to_pdf;
use hyperlab_persistence::load;
use hyperlab_stack::{Object, PartContainer, Stack};

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate is two levels below the repository root")
        .join("examples")
}

fn open(name: &str) -> Stack {
    load(examples().join(format!("{name}.hl"))).unwrap_or_else(|error| panic!("{name}: {error}"))
}

/// How many pages the document says it has.
fn pages(pdf: &[u8]) -> usize {
    // `/Count n` in the page tree. There is exactly one, because there is one
    // page tree and it has no sub-trees.
    let text = String::from_utf8_lossy(pdf);
    let at = text.find("/Count ").expect("a page tree with a count");
    text[at + 7..]
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|digits| digits.parse().ok())
        .expect("the count is a number")
}

#[test]
fn every_example_becomes_a_pdf_with_a_page_per_card() {
    for name in [
        "Address Book",
        "Recipe Box",
        "Todo",
        "Cluedo",
        "Myst",
        "Language Models, Explained",
    ] {
        let stack = open(name);
        let pdf = to_pdf(&stack).unwrap_or_else(|error| panic!("{name}: {error}"));

        assert!(pdf.starts_with(b"%PDF-"), "{name} is not a PDF");
        assert!(
            pdf.ends_with(b"%%EOF\n") || pdf.ends_with(b"%%EOF"),
            "{name} is truncated"
        );
        assert_eq!(pages(&pdf), stack.card_count(), "{name}: a page per card");
    }
}

#[test]
fn the_words_on_a_card_are_words_in_the_document() {
    // Not a picture of text: a reader can search this, and so can this test.
    let pdf = to_pdf(&open("Todo")).expect("Todo exports");
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("Things To Do"), "the caption is missing");
    assert!(text.contains("Clear Done"), "a button's label is missing");
    assert!(
        text.contains("/BaseFont /Helvetica"),
        "the text should use a font every reader already has"
    );
}

#[test]
fn a_dash_written_into_a_card_survives_into_the_document() {
    // The strings are WinAnsi, and a reader told nothing assumes the Standard
    // encoding — which leaves the byte an em dash is written as unassigned, so
    // the character draws as nothing at all. It went unnoticed for as long as
    // no example used one.
    let mut stack = hyperlab_stack::Stack::new("Punctuation");
    let card = stack.cards()[0].id();
    let mut caption = stack.new_part(
        hyperlab_stack::PartKind::Field,
        "Caption",
        hyperlab_stack::Rect::new(10, 10, 200, 40),
    );
    caption
        .set_property("text", "one \u{2014} two".into())
        .expect("text is an ordinary property");
    stack
        .card_mut(card)
        .expect("the card exists")
        .add_part(caption);

    let pdf = to_pdf(&stack).expect("it exports");
    let text = String::from_utf8_lossy(&pdf);
    assert!(
        text.contains("/Encoding /WinAnsiEncoding"),
        "the strings are WinAnsi and the font has to say so"
    );
    // Strings are written as hex, so this is "one \x97 two" spelled out.
    assert!(
        text.contains("<6F6E6520972074776F>"),
        "the dash did not reach the page"
    );
}

#[test]
fn a_stack_that_carries_pictures_carries_them_into_the_document() {
    let stack = open("Cluedo");
    assert!(!stack.images().is_empty(), "Cluedo is the one with artwork");

    let pdf = to_pdf(&stack).expect("Cluedo exports");
    let text = String::from_utf8_lossy(&pdf);
    // Vector artwork, not a photograph of it: the pictures arrive as form
    // XObjects with their own content, and every page that draws one names it.
    assert!(
        text.contains("/Subtype /Form"),
        "the artwork was not embedded"
    );
    assert!(text.contains("/XObject"), "no page refers to a picture");
}

#[test]
fn an_empty_stack_is_still_a_document() {
    // One card, nothing on it. The awkward case for anything that assumes a
    // page has content.
    let stack = Stack::new("Nothing");
    let pdf = to_pdf(&stack).expect("an empty stack exports");
    assert!(pdf.starts_with(b"%PDF-"));
    assert_eq!(pages(&pdf), 1);
}

#[test]
fn a_hidden_part_is_not_printed() {
    // Hidden is faint in the editor and absent on paper. `Myst` has none, so
    // the check is that hiding something removes it.
    let mut stack = open("Todo");
    let card = stack.cards()[0].id();
    let part = stack
        .card(card)
        .and_then(|card| {
            hyperlab_stack::PartContainer::parts(card)
                .iter()
                .find(|part| part.name() == "Items")
        })
        .map(Object::id)
        .expect("Todo has an Items field");

    let before = to_pdf(&stack).expect("exports");
    stack
        .card_mut(card)
        .and_then(|card| hyperlab_stack::PartContainer::part_mut(card, part))
        .expect("the part is there")
        .set_property("visible", false.into())
        .expect("visible is an ordinary property");
    let after = to_pdf(&stack).expect("still exports");

    assert!(
        after.len() < before.len(),
        "hiding a field should take its text off the page"
    );
    assert!(!String::from_utf8_lossy(&after).contains("read the HyperTalk reference"));
}
