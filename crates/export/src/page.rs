//! One page per card.
//!
//! A card's coordinates run down from the top left; a PDF's run up from the
//! bottom left. Everything here goes through [`Frame::up`], which is the only
//! place that flip happens.

use std::collections::BTreeMap;

use hyperlab_stack::{Object, PartContainer, PartKind, Rect, Size, Stack, Value};
use pdf_writer::{Content, Finish, Name, Pdf, Ref, Str, types::LineCapStyle};

use crate::{ExportError, art, metrics};

/// The size text is drawn at, matching the renderer's `--text`.
const TEXT: f32 = 13.0;

/// How far apart lines sit, as a multiple of the text size.
const LEADING: f32 = 1.35;

/// The renderer's `--space-tight`, which is a field's left inset.
const INSET: f32 = 4.0;

/// The renderer's `--radius-button`.
const RADIUS: f32 = 9.0;

/// Everything a page needs to know about which way is up.
struct Frame {
    height: f32,
}

impl Frame {
    /// A card `y`, as a PDF `y`.
    const fn up(&self, y: f32) -> f32 {
        self.height - y
    }
}

/// Builds the document.
pub fn document(stack: &Stack) -> Result<Vec<u8>, ExportError> {
    let Size { width, height } = stack.size();
    let (width, height) = (width as f32, height as f32);
    let frame = Frame { height };

    let mut pdf = Pdf::new();
    let mut next = 1;
    let mut claim = || {
        let id = Ref::new(next);
        next += 1;
        id
    };

    let catalog = claim();
    let tree = claim();
    let font = claim();

    // Every picture is converted once, however many cards draw it, and given a
    // name the pages share.
    let mut drawn: BTreeMap<String, art::Art> = BTreeMap::new();
    for (name, picture) in stack.images() {
        drawn.insert(name.clone(), art::render(picture, &mut next)?);
    }

    let mut pages = Vec::new();
    let mut streams = Vec::new();
    for card in stack.cards() {
        let page = Ref::new(next);
        next += 1;
        let content = Ref::new(next);
        next += 1;

        let mut ink = Content::new();
        // The card itself: white, with the border the window draws round it.
        ink.set_fill_gray(1.0);
        ink.rect(0.0, 0.0, width, height);
        ink.fill_nonzero();

        // Background parts first, then the card's own, which is the order the
        // renderer stacks them in.
        let background = stack.background_of(card.id());
        let parts = background
            .map(PartContainer::parts)
            .unwrap_or_default()
            .iter()
            .chain(card.parts().iter());

        let mut used: Vec<(String, Ref)> = Vec::new();
        for part in parts {
            draw(part, &frame, &mut ink, &drawn, &mut used);
        }

        pages.push((page, content, used));
        streams.push((content, ink.finish()));
    }

    pdf.catalog(catalog).pages(tree);
    let kids: Vec<Ref> = pages.iter().map(|(page, _, _)| *page).collect();
    pdf.pages(tree)
        .count(i32::try_from(kids.len()).unwrap_or(i32::MAX))
        .kids(kids);
    // [`crate::encode`] spells strings in WinAnsi, and a reader told nothing
    // assumes the Standard encoding instead — where the bytes curly quotes and
    // dashes live at are unassigned, so those characters silently draw as
    // nothing. Saying which encoding it is costs one entry.
    pdf.type1_font(font)
        .base_font(Name(b"Helvetica"))
        .encoding_predefined(Name(b"WinAnsiEncoding"));

    for (page, content, used) in &pages {
        let mut written = pdf.page(*page);
        written
            .media_box(pdf_writer::Rect::new(0.0, 0.0, width, height))
            .parent(tree)
            .contents(*content);
        let mut resources = written.resources();
        resources.fonts().pair(Name(b"F1"), font);
        let mut objects = resources.x_objects();
        for (name, id) in used {
            objects.pair(Name(name.as_bytes()), *id);
        }
        objects.finish();
        resources.finish();
        written.finish();
    }

    for (content, stream) in streams {
        pdf.stream(content, &stream);
    }
    for picture in drawn.into_values() {
        pdf.extend(&picture.chunk);
    }

    Ok(pdf.finish())
}

/// Draws one part, if it is drawn at all.
fn draw(
    part: &hyperlab_stack::Part,
    frame: &Frame,
    ink: &mut Content,
    drawn: &BTreeMap<String, art::Art>,
    used: &mut Vec<(String, Ref)>,
) {
    let flag = |name: &str, fallback: bool| {
        part.property(name)
            .and_then(|value| value.as_bool())
            .unwrap_or(fallback)
    };
    // A hidden part is faint in the editor and absent from a printout, which
    // is what "hidden" means to someone reading the paper.
    if !flag("visible", true) {
        return;
    }

    let rect: Rect = part.geometry();
    let style = part
        .property("style")
        .unwrap_or(Value::Empty)
        .as_text()
        .to_ascii_lowercase();

    match part.part_kind() {
        PartKind::Image => {
            let source = part.property("source").unwrap_or(Value::Empty).as_text();
            let Some(picture) = drawn.get(&source) else {
                return;
            };
            // Named per page by position, because a name has to be a PDF name
            // and a picture is called things like "clock-tower.svg".
            let name = format!("X{}", used.len());
            used.push((name.clone(), picture.id));

            ink.save_state();
            // An XObject draws into the unit square, so the matrix is the
            // rectangle it has been given.
            ink.transform([
                rect.width as f32,
                0.0,
                0.0,
                rect.height as f32,
                rect.left as f32,
                frame.up((rect.top + rect.height) as f32),
            ]);
            ink.x_object(Name(name.as_bytes()));
            ink.restore_state();
        }
        PartKind::Button => {
            if style != "transparent" {
                box_of(
                    ink,
                    frame,
                    rect,
                    if style == "rectangle" { 0.0 } else { RADIUS },
                );
            }
            if flag("showName", true) {
                centred(ink, frame, rect, part.name());
            }
        }
        PartKind::Field => {
            if style == "shadow" {
                // The shadow first, so the box lands on top of it.
                let behind = Rect::new(rect.left + 3, rect.top + 3, rect.width, rect.height);
                ink.set_fill_gray(0.5);
                ink.rect(
                    behind.left as f32,
                    frame.up((behind.top + behind.height) as f32),
                    behind.width as f32,
                    behind.height as f32,
                );
                ink.fill_nonzero();
            }
            if style != "transparent" {
                box_of(ink, frame, rect, 0.0);
            }
            paragraph(ink, frame, rect, &part.text());
        }
    }
}

/// A white box with a black edge, optionally with rounded corners.
fn box_of(ink: &mut Content, frame: &Frame, rect: Rect, radius: f32) {
    let (left, width) = (rect.left as f32, rect.width as f32);
    let (bottom, height) = (
        frame.up((rect.top + rect.height) as f32),
        rect.height as f32,
    );

    ink.set_fill_gray(1.0);
    ink.set_stroke_gray(0.0);
    ink.set_line_width(1.0);
    ink.set_line_cap(LineCapStyle::ButtCap);

    let radius = radius.min(width / 2.0).min(height / 2.0);
    if radius <= 0.0 {
        ink.rect(left, bottom, width, height);
        ink.fill_even_odd_and_stroke();
        return;
    }

    // A rounded rectangle, drawn as four lines and four Bézier corners. 0.5523
    // is the constant that makes a cubic curve look like a quarter circle.
    let pull = radius * 0.552_284_8;
    let (right, top) = (left + width, bottom + height);
    ink.move_to(left + radius, bottom);
    ink.line_to(right - radius, bottom);
    ink.cubic_to(
        right - radius + pull,
        bottom,
        right,
        bottom + radius - pull,
        right,
        bottom + radius,
    );
    ink.line_to(right, top - radius);
    ink.cubic_to(
        right,
        top - radius + pull,
        right - radius + pull,
        top,
        right - radius,
        top,
    );
    ink.line_to(left + radius, top);
    ink.cubic_to(
        left + radius - pull,
        top,
        left,
        top - radius + pull,
        left,
        top - radius,
    );
    ink.line_to(left, bottom + radius);
    ink.cubic_to(
        left,
        bottom + radius - pull,
        left + radius - pull,
        bottom,
        left + radius,
        bottom,
    );
    ink.close_path();
    ink.fill_even_odd_and_stroke();
}

/// A field's text, wrapped into its box and clipped to it.
fn paragraph(ink: &mut Content, frame: &Frame, rect: Rect, text: &str) {
    if text.is_empty() {
        return;
    }
    let room = rect.width as f32 - INSET * 2.0;
    if room <= 0.0 {
        return;
    }

    ink.save_state();
    // Clipped, because the renderer gives every part `overflow: hidden` and a
    // field with more text than box should not spill across the card.
    ink.rect(
        rect.left as f32,
        frame.up((rect.top + rect.height) as f32),
        rect.width as f32,
        rect.height as f32,
    );
    ink.clip_nonzero();
    ink.end_path();

    ink.set_fill_gray(0.0);
    ink.begin_text();
    ink.set_font(Name(b"F1"), TEXT);
    ink.set_leading(TEXT * LEADING);
    // The first baseline sits one line below the top edge, plus the hair of
    // padding the renderer puts there.
    ink.next_line(
        rect.left as f32 + INSET,
        frame.up(rect.top as f32 + TEXT + 2.0),
    );
    for (index, line) in metrics::wrap(text, TEXT, room).into_iter().enumerate() {
        if index > 0 {
            ink.next_line(0.0, -TEXT * LEADING);
        }
        ink.show(Str(&line));
    }
    ink.end_text();
    ink.restore_state();
}

/// A button's label, centred in it.
fn centred(ink: &mut Content, frame: &Frame, rect: Rect, label: &str) {
    let encoded = metrics::encode(label);
    let across = metrics::width(&encoded, TEXT);
    let left = rect.left as f32 + (rect.width as f32 - across) / 2.0;
    // Optically centred: half the height, less about a third of the text size,
    // which puts the middle of a capital on the middle of the box.
    let baseline = frame.up(rect.top as f32 + rect.height as f32 / 2.0 + TEXT * 0.35);

    ink.save_state();
    ink.set_fill_gray(0.0);
    ink.begin_text();
    ink.set_font(Name(b"F1"), TEXT);
    ink.next_line(left.max(rect.left as f32 + 2.0), baseline);
    ink.show(Str(&encoded));
    ink.end_text();
    ink.restore_state();
}
