//! Describing a stack to a model.
//!
//! The runtime must not know what a prompt looks like, and a model cannot be
//! handed a `Stack`. This module is the join: it reads the object model and
//! writes a plain description a model can follow.
//!
//! What goes in the description is a deliberate, reviewable decision, because
//! it is exactly what leaves the user's machine. Nothing is included that the
//! user could not see by opening the inspector.

use hyperlab_stack::{Id, Object, PartContainer, Stack};

/// How much of a stack to describe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextOptions {
    /// Whether to include the text held in fields. Off by default: a stack's
    /// contents are the user's data, and a question about a *script* does not
    /// need them.
    pub include_field_text: bool,
    /// Whether to include scripts.
    pub include_scripts: bool,
    /// How much of any one script or field to include, in characters.
    pub max_text: usize,
}

impl Default for ContextOptions {
    fn default() -> Self {
        Self {
            include_field_text: false,
            include_scripts: true,
            max_text: 2_000,
        }
    }
}

impl ContextOptions {
    /// Describes everything, for a question about the user's data.
    #[must_use]
    pub fn everything() -> Self {
        Self {
            include_field_text: true,
            include_scripts: true,
            max_text: 8_000,
        }
    }
}

/// Describes the current card and the stack around it.
///
/// The result is Markdown, because every model reads it well and because a
/// person can check what was sent by looking at it.
#[must_use]
pub fn describe_card(stack: &Stack, card_id: Id, options: ContextOptions) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Stack \"{}\"\n\n- {} card(s), {} background(s)\n- card size: {}×{}\n",
        stack.name(),
        stack.card_count(),
        stack.backgrounds().len(),
        stack.size().width,
        stack.size().height
    ));
    if options.include_scripts && !stack.script().trim().is_empty() {
        out.push_str(&code_block(
            "stack script",
            stack.script(),
            options.max_text,
        ));
    }

    let Some(card) = stack.card(card_id) else {
        out.push_str("\nThe current card no longer exists.\n");
        return out;
    };

    let position = stack.card_index(card_id).map_or(0, |index| index + 1);
    out.push_str(&format!(
        "\n## Card {position} of {}: \"{}\" (id {})\n",
        stack.card_count(),
        card.name(),
        card.id()
    ));
    describe_parts(&mut out, card, options);
    if options.include_scripts && !card.script().trim().is_empty() {
        out.push_str(&code_block("card script", card.script(), options.max_text));
    }

    if let Some(background) = stack.background_of(card_id) {
        out.push_str(&format!(
            "\n## Background \"{}\" (id {}), shared by every card that uses it\n",
            background.name(),
            background.id()
        ));
        describe_parts(&mut out, background, options);
        if options.include_scripts && !background.script().trim().is_empty() {
            out.push_str(&code_block(
                "background script",
                background.script(),
                options.max_text,
            ));
        }
    }
    out
}

/// Lists every card, for questions about the stack as a whole.
#[must_use]
pub fn describe_stack_outline(stack: &Stack) -> String {
    let mut out = format!("# Stack \"{}\"\n\n", stack.name());
    for (index, card) in stack.cards().iter().enumerate() {
        out.push_str(&format!(
            "{}. \"{}\" (id {}) — {} button(s), {} field(s)\n",
            index + 1,
            card.name(),
            card.id(),
            card.parts_of_kind(hyperlab_stack::PartKind::Button).len(),
            card.parts_of_kind(hyperlab_stack::PartKind::Field).len(),
        ));
    }
    out
}

fn describe_parts<T: PartContainer>(out: &mut String, container: &T, options: ContextOptions) {
    if container.parts().is_empty() {
        out.push_str("\n(no buttons or fields)\n");
        return;
    }
    for part in container.parts() {
        let geometry = part.geometry();
        out.push_str(&format!(
            "\n- {} \"{}\" (id {}) at {},{} {}×{}\n",
            part.kind(),
            part.name(),
            part.id(),
            geometry.left,
            geometry.top,
            geometry.width,
            geometry.height
        ));
        if options.include_field_text && part.part_kind() == hyperlab_stack::PartKind::Field {
            let text = part.text();
            if !text.is_empty() {
                out.push_str(&format!("  contents: {}\n", truncate(&text, 200)));
            }
        }
        if options.include_scripts && !part.script().trim().is_empty() {
            out.push_str(&code_block(
                &format!("script of {} \"{}\"", part.kind(), part.name()),
                part.script(),
                options.max_text,
            ));
        }
    }
}

fn code_block(title: &str, source: &str, max: usize) -> String {
    format!(
        "\n{title}:\n```hypertalk\n{}\n```\n",
        truncate(source.trim(), max)
    )
}

/// Cuts text to a length, saying so rather than trimming silently.
fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max).collect();
    format!(
        "{kept}\n… ({} characters omitted)",
        text.chars().count() - max
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyperlab_stack::{PartKind, Rect, Value};

    fn stack() -> Stack {
        let mut stack = Stack::new("Recipes");
        let card = stack.cards()[0].id();
        let mut field = stack.new_part(PartKind::Field, "Notes", Rect::new(1, 2, 3, 4));
        field
            .set_property("text", Value::text("secret sauce"))
            .unwrap();
        let mut button = stack.new_part(PartKind::Button, "Go", Rect::default());
        button.set_script("on mouseUp\n  go to next card\nend mouseUp");
        stack.card_mut(card).unwrap().add_part(field);
        stack.card_mut(card).unwrap().add_part(button);
        stack
    }

    #[test]
    fn field_contents_are_left_out_unless_asked_for() {
        let stack = stack();
        let card = stack.cards()[0].id();
        let described = describe_card(&stack, card, ContextOptions::default());
        assert!(!described.contains("secret sauce"), "{described}");

        let described = describe_card(&stack, card, ContextOptions::everything());
        assert!(described.contains("secret sauce"));
    }

    #[test]
    fn scripts_and_geometry_are_described() {
        let stack = stack();
        let card = stack.cards()[0].id();
        let described = describe_card(&stack, card, ContextOptions::default());
        assert!(described.contains("go to next card"));
        assert!(described.contains("1,2 3×4"));
    }

    #[test]
    fn long_text_is_cut_and_says_so() {
        let mut stack = stack();
        let card = stack.cards()[0].id();
        let long = "x".repeat(500);
        stack.card_mut(card).unwrap().parts_mut()[1].set_script(&long);

        let options = ContextOptions {
            max_text: 100,
            ..ContextOptions::default()
        };
        let described = describe_card(&stack, card, options);
        assert!(described.contains("characters omitted"), "{described}");
    }

    #[test]
    fn the_outline_lists_every_card() {
        let mut stack = stack();
        let background = stack.backgrounds()[0].id();
        let second = stack.new_card(background).unwrap();
        stack.add_card(second);

        let outline = describe_stack_outline(&stack);
        assert!(outline.contains("1. \"Card 1\""));
        assert!(outline.contains("2. \"Card 2\""));
    }
}
