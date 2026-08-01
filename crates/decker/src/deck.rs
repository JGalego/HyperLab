//! The line-oriented text a `.deck` file is made of.
//!
//! ```text
//! {deck}
//! version:1
//! card:0
//! size:[640,460]
//!
//! {card:themansion}
//! image:"%%IMG0…"
//! {widgets}
//! ask:{"type":"button","size":[100,24],"pos":[452,240],"script":"themansion.0","text":"Ask","style":"rect"}
//!
//! {script:themansion.0}
//! on click do
//!  alert["…"]
//! end
//! {end}
//! ```
//!
//! Every rule here was checked by writing a deck and opening it in Decker: a
//! field's contents live in `value` while a script reads them as `.text`, a
//! button's label is `text`, and a card's artwork has to be the size of the
//! whole deck.

use std::collections::{BTreeMap, HashSet};

use hyperlab_stack::{Object, PartContainer, PartKind, Rect, Size, Stack, Value};

use crate::{Translation, image::Sheet, lil};

/// What a stack becomes.
#[must_use]
pub fn deck(stack: &Stack) -> Translation {
    let Size { width, height } = stack.size();
    let mut notes = Vec::new();
    let mut out = String::new();

    let names: Vec<String> = unique(stack.cards().iter().enumerate().map(|(at, card)| {
        if card.name().is_empty() {
            format!("card{}", at + 1)
        } else {
            identifier(card.name())
        }
    }));
    let known: HashSet<String> = stack
        .cards()
        .iter()
        .filter(|card| !card.name().is_empty())
        .map(|card| card.name().to_ascii_lowercase())
        .collect();

    // A script can reach a widget on another card, so every card's widgets are
    // named before any script is translated.
    let everywhere: BTreeMap<String, Widgets> = stack
        .cards()
        .iter()
        .enumerate()
        .filter(|(_, card)| !card.name().is_empty())
        .map(|(at, card)| {
            (card.name().to_ascii_lowercase(), {
                let parts = parts_of(stack, card);
                let widget_names = unique(parts.iter().map(|part| identifier(part.name())));
                Widgets {
                    card: names[at].clone(),
                    parts: named(&parts, &widget_names),
                }
            })
        })
        .collect();

    // The deck's own block is finished last: whether it names a script is not
    // known until the stack's script has been looked at.
    let mut header = format!(
        "{{deck}}\nversion:1\ncard:0\nsize:[{width},{height}]\nname:{}\n",
        string(stack.name())
    );

    for (index, card) in stack.cards().iter().enumerate() {
        let card_name = &names[index];
        let parts = parts_of(stack, card);
        let widget_names = unique(parts.iter().map(|part| identifier(part.name())));
        let here = Here {
            here: Widgets {
                card: card_name.clone(),
                parts: named(&parts, &widget_names),
            },
            everywhere: &everywhere,
            cards: &known,
        };

        // The artwork is one bitmap behind the widgets, which is where a deck
        // keeps a picture that is not itself interactive.
        let mut sheet = Sheet::new(width, height);
        for part in &parts {
            if part.part_kind() != PartKind::Image {
                continue;
            }
            let source = part.property("source").unwrap_or(Value::Empty).as_text();
            let Some(picture) = stack.image(&source) else {
                notes.push(format!("the picture \"{source}\" is not in the stack"));
                continue;
            };
            let Some(sheet) = sheet.as_mut() else {
                continue;
            };
            if !sheet.draw(picture, part.geometry()) {
                notes.push(format!("the picture \"{source}\" could not be drawn"));
            }
        }

        out.push_str(&format!("\n{{card:{card_name}}}\n"));
        if let Some(record) = sheet.as_ref().and_then(Sheet::finish) {
            out.push_str(&format!("image:\"{record}\"\n"));
        }

        // Scripts are numbered per card and written after the widgets.
        let mut scripts = Vec::new();
        let mut widgets = String::new();
        for (part, name) in parts.iter().zip(widget_names.iter()) {
            let (handled, said) = lil::handlers(part.script(), &here);
            notes.extend(said);
            let script = handled.map(|one| {
                let id = format!("{card_name}.{}", scripts.len());
                scripts.push((id.clone(), one.source));
                id
            });
            if let Some(line) = widget(part, name, script.as_deref()) {
                widgets.push_str(&line);
                widgets.push('\n');
            }
        }

        let (card_script, said) = lil::handlers(card.script(), &here);
        notes.extend(said);
        if let Some(one) = card_script {
            let id = format!("{card_name}.{}", scripts.len());
            out.push_str(&format!("script:\"{id}\"\n"));
            scripts.push((id, one.source));
        }

        out.push_str("{widgets}\n");
        out.push_str(&widgets);
        for (id, source) in scripts {
            out.push_str(&format!("\n{{script:{id}}}\n{source}\n{{end}}\n"));
        }
    }

    // A deck has a script of its own, and it sits at the end of the message
    // path just as a stack's does: a handler on a card shadows one here.
    let (stack_script, said) = lil::handlers(
        stack.script(),
        &Here {
            here: Widgets {
                card: String::new(),
                parts: BTreeMap::new(),
            },
            everywhere: &everywhere,
            cards: &known,
        },
    );
    notes.extend(said);
    if let Some(one) = stack_script {
        out.push_str(&format!("\n{{script:deck}}\n{}\n{{end}}\n", one.source));
        header.push_str("script:\"deck\"\n");
    }

    Translation {
        source: format!(
            "<meta charset=\"UTF-8\"><body><script language=\"decker\">\n{header}{out}\n</script>\n"
        ),
        notes,
    }
}

/// One card's name, and what each of its parts is called on it.
struct Widgets {
    card: String,
    parts: BTreeMap<(String, String), String>,
}

impl Widgets {
    fn part(&self, kind: &str, name: &str) -> Option<&String> {
        self.parts
            .get(&(kind.to_string(), name.to_ascii_lowercase()))
    }
}

/// What a script being translated can see: its own card, and the rest.
struct Here<'a> {
    here: Widgets,
    everywhere: &'a BTreeMap<String, Widgets>,
    cards: &'a HashSet<String>,
}

impl lil::Widgets for Here<'_> {
    fn named(&self, kind: &str, name: &str) -> Option<String> {
        self.here.part(kind, name).cloned()
    }

    fn elsewhere(&self, card: &str, kind: &str, name: &str) -> Option<(String, String)> {
        let there = self.everywhere.get(&card.to_ascii_lowercase())?;
        Some((there.card.clone(), there.part(kind, name)?.clone()))
    }

    fn card(&self, name: &str) -> bool {
        self.cards.contains(&name.to_ascii_lowercase())
    }
}

/// Every part on a card, the background's first, as the runtime looks them up.
fn parts_of<'a>(stack: &'a Stack, card: &'a hyperlab_stack::Card) -> Vec<&'a hyperlab_stack::Part> {
    stack
        .background_of(card.id())
        .map(PartContainer::parts)
        .unwrap_or_default()
        .iter()
        .chain(card.parts().iter())
        .collect()
}

/// Parts against the widget names they were given.
fn named(
    parts: &[&hyperlab_stack::Part],
    widget_names: &[String],
) -> BTreeMap<(String, String), String> {
    parts
        .iter()
        .zip(widget_names.iter())
        .map(|(part, name)| {
            (
                (
                    part.kind().as_str().to_string(),
                    part.name().to_ascii_lowercase(),
                ),
                name.clone(),
            )
        })
        .collect()
}

/// One widget line, or `None` for a part a deck has no widget for.
fn widget(part: &hyperlab_stack::Part, name: &str, script: Option<&str>) -> Option<String> {
    let flag = |property: &str, fallback: bool| {
        part.property(property)
            .and_then(|value| value.as_bool())
            .unwrap_or(fallback)
    };
    if !flag("visible", true) {
        return None;
    }
    // A picture is painted into the card's artwork, so it needs a widget only
    // to be clicked on, and then an invisible button is the whole of it.
    let picture = part.part_kind() == PartKind::Image;
    let script = if picture { Some(script?) } else { script };

    let rect: Rect = part.geometry();
    let mut fields = vec![
        match part.part_kind() {
            PartKind::Button | PartKind::Image => "\"type\":\"button\"".to_string(),
            PartKind::Field => "\"type\":\"field\"".to_string(),
        },
        format!("\"size\":[{},{}]", rect.width, rect.height),
        format!("\"pos\":[{},{}]", rect.left, rect.top),
    ];
    let style = part
        .property("style")
        .unwrap_or(Value::Empty)
        .as_text()
        .to_ascii_lowercase();
    match part.part_kind() {
        PartKind::Image => fields.push("\"style\":\"invisible\"".to_string()),
        PartKind::Button => {
            fields.push(format!(
                "\"style\":\"{}\"",
                if style == "transparent" {
                    "invisible"
                } else {
                    "rect"
                }
            ));
            if flag("showName", true) {
                fields.push(format!("\"text\":{}", string(part.name())));
            }
        }
        PartKind::Field => {
            if flag("locked", false) {
                fields.push("\"locked\":1".to_string());
            }
            if style == "transparent" {
                fields.push("\"border\":0".to_string());
            }
            let text = part.text();
            if !text.is_empty() {
                fields.push(format!("\"value\":{}", string(&text)));
            }
        }
    }
    if let Some(script) = script {
        fields.push(format!("\"script\":\"{script}\""));
    }

    Some(format!("{name}:{{{}}}", fields.join(",")))
}

/// A name a deck can use: lower case, letters and digits only.
///
/// Decker looks widgets and cards up by these, and a script says them, so the
/// same rule has to be applied in both places.
#[must_use]
pub fn identifier(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_lowercase())
        .collect();
    if cleaned.is_empty() || cleaned.starts_with(|c: char| c.is_ascii_digit()) {
        format!("w{cleaned}")
    } else {
        cleaned
    }
}

/// Makes every name in the list different from the others, in order.
fn unique(names: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    names
        .map(|name| {
            let count = seen.entry(name.clone()).or_default();
            *count += 1;
            if *count == 1 {
                name
            } else {
                format!("{name}{count}")
            }
        })
        .collect()
}

/// A string as the text format writes one.
///
/// A forward slash begins a comment, so it is escaped everywhere — the same
/// rule that keeps base64 artwork intact.
fn string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '/' => out.push_str("\\/"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use hyperlab_stack::{PartKind, Rect, Stack};

    use super::*;

    #[test]
    fn a_deck_says_what_it_is_before_anything_else() {
        let written = deck(&Stack::new("Empty")).source;
        assert!(written.starts_with("<meta charset=\"UTF-8\"><body><script language=\"decker\">"));
        assert!(written.contains("\n{deck}\nversion:1\ncard:0\n"));
        assert!(written.trim_end().ends_with("</script>"));
    }

    #[test]
    fn two_parts_with_one_name_get_two_widget_names() {
        // Decker looks a widget up by name, so a duplicate would hide one.
        assert_eq!(
            unique(["a".to_string(), "a".to_string(), "b".to_string()].into_iter()),
            vec!["a", "a2", "b"]
        );
    }

    #[test]
    fn a_slash_never_reaches_the_file_unescaped() {
        // It begins a comment, and would cut the rest of the line away.
        assert_eq!(string("and/or"), "\"and\\/or\"");
    }

    #[test]
    fn a_field_carries_its_text_and_a_button_its_label() {
        let mut stack = Stack::new("X");
        let card = stack.cards()[0].id();
        let mut field = stack.new_part(PartKind::Field, "Notes", Rect::new(1, 2, 30, 40));
        field
            .set_property("text", "hello".into())
            .expect("ordinary");
        let button = stack.new_part(PartKind::Button, "Press", Rect::new(5, 6, 70, 20));
        let holder = stack.card_mut(card).expect("the card exists");
        holder.add_part(field);
        holder.add_part(button);

        let written = deck(&stack).source;
        assert!(written.contains("notes:{\"type\":\"field\""), "{written}");
        assert!(written.contains("\"value\":\"hello\""), "{written}");
        assert!(written.contains("press:{\"type\":\"button\""), "{written}");
        assert!(written.contains("\"text\":\"Press\""), "{written}");
        assert!(written.contains("\"size\":[30,40]"), "{written}");
        assert!(written.contains("\"pos\":[5,6]"), "{written}");
    }

    #[test]
    fn a_script_is_a_chunk_the_widget_points_at() {
        let mut stack = Stack::new("X");
        let card = stack.cards()[0].id();
        let mut button = stack.new_part(PartKind::Button, "Press", Rect::new(0, 0, 10, 10));
        button.set_script("on mouseUp\n  answer \"hi\"\nend mouseUp");
        stack
            .card_mut(card)
            .expect("the card exists")
            .add_part(button);

        let written = deck(&stack).source;
        assert!(written.contains("\"script\":\"card1.0\""), "{written}");
        assert!(written.contains("{script:card1.0}"), "{written}");
        assert!(written.contains("on click do"), "{written}");
        assert!(written.contains("\n{end}\n"), "{written}");
    }
}
