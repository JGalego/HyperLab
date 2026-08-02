//! A stack whose artwork carries words still exports once a font is
//! registered.
//!
//! What this can prove on a desktop is that registering a font breaks
//! nothing and the words are drawn. It cannot prove the *registered* font is
//! what drew them, because a desktop has its own and usvg is welcome to
//! prefer those. The case that needs the registry — a browser, where there
//! are no system fonts at all — is checked by driving the real page.

use hyperlab_export::{add_font, to_pdf};
use hyperlab_stack::{Image, Object, PartContainer, PartKind, Rect, Stack};

/// A drawing whose only content is a word, so a PDF that has the word came
/// by it through the typeface rather than by accident.
const LABELLED: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 60">
  <text x="10" y="40" font-family="Chicago,Geneva,sans-serif" font-size="24">Kitchen</text>
</svg>"#;

/// The font every desktop this runs on happens to have. Skipped where it is
/// absent rather than failing: the point is the registry, not the file.
const CANDIDATES: [&str; 3] = [
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
];

fn a_font() -> Option<Vec<u8>> {
    CANDIDATES.iter().find_map(|path| std::fs::read(path).ok())
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
fn artwork_with_words_exports_with_a_font_registered() {
    let Some(font) = a_font() else {
        eprintln!("no font on this machine to register; skipping");
        return;
    };

    add_font(font);
    let pdf = to_pdf(&stack_with_art()).expect("the stack exports");

    // A PDF that drew the word embeds the subset it needed, so the font's
    // name appears in the file. Without any font, usvg drops the text node
    // and nothing of the sort is written.
    let haystack = String::from_utf8_lossy(&pdf);
    assert!(
        haystack.contains("FontFile")
            || haystack.contains("Type0")
            || haystack.contains("TrueType"),
        "a picture's text should have been drawn with an embedded face"
    );
}
