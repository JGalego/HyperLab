//! A stack whose artwork carries words exports, with a font registered.
//!
//! What this can check on any machine is that registering a font breaks
//! nothing and the document still comes out whole. It deliberately does not
//! assert that the words were *drawn*: whether they are depends on the fonts
//! the machine happens to have, and a test that asserts otherwise passes on
//! a developer's laptop and fails on a bare CI runner — which is exactly
//! what an earlier version of this file did.
//!
//! The two claims that matter are checked where they can be checked
//! honestly: that a registered font reaches the converter, by the unit tests
//! in `fonts.rs`; and that a browser — which has no system fonts at all —
//! ends up with the labels in the PDF, by driving the real page.

use hyperlab_export::{add_font, to_pdf};
use hyperlab_stack::{Image, Object, PartContainer, PartKind, Rect, Stack};

/// A drawing whose only content is a word.
const LABELLED: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 60">
  <text x="10" y="40" font-family="Chicago,Geneva,sans-serif" font-size="24">Kitchen</text>
</svg>"#;

/// Whatever font this machine has, if it has one at all.
fn a_font() -> Option<Vec<u8>> {
    [
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
    ]
    .iter()
    .find_map(|path| std::fs::read(path).ok())
}

fn stack_with_art() -> Stack {
    let mut stack = Stack::new("Labelled");
    let image = Image::new("label.svg", LABELLED.as_bytes().to_vec()).expect("valid SVG");
    stack.set_image("label.svg", Some(image));

    let card = stack.cards()[0].id();
    let mut part = stack.new_part(PartKind::Image, "label.svg", Rect::new(10, 10, 200, 60));
    part.set_property("source", hyperlab_stack::Value::text("label.svg"))
        .expect("source is a property of an image part");
    stack
        .card_mut(card)
        .expect("it was just listed")
        .add_part(part);
    stack
}

#[test]
fn artwork_with_words_still_exports_when_a_font_is_registered() {
    if let Some(font) = a_font() {
        add_font(font);
    }

    let pdf = to_pdf(&stack_with_art()).expect("the stack exports");

    assert!(pdf.starts_with(b"%PDF-"), "that is not a PDF");
    assert!(
        pdf.windows(5).any(|window| window == b"%%EOF"),
        "the document was never finished"
    );
}
