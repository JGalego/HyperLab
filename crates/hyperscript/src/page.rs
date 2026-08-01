//! A stack as one HTML file.
//!
//! Cards become sections, parts become elements positioned exactly where the
//! renderer puts them, and every script becomes an `_` attribute. What is left
//! over — moving between cards, and the card messages HyperLab sends — is a
//! dozen lines of glue at the bottom of the page.
//!
//! The result needs no HyperLab and no server. It needs _hyperscript, which it
//! fetches from a CDN, and that is the one thing in it that will not work
//! without a network.

use std::collections::BTreeMap;

use hyperlab_stack::{Object, PartContainer, PartKind, Rect, Size, Stack, Value};

use crate::{
    Translation,
    html::{escape, slug},
    script::{Elements, handlers},
};

/// The library, and the hash that says it is the library.
const LIBRARY: &str =
    "https://cdn.jsdelivr.net/npm/hyperscript.org@0.9.93/dist/_hyperscript.min.js";
const INTEGRITY: &str = "sha384-/6HsqTiz02YfFBUhzTwlH/yxe68DhfnkdHiWytM3nxAzs/yvG+3FZY0f4KLnNoov";

/// Where everything on the page lives, worked out before anything is written.
///
/// Ids have to exist before the scripts that name them are translated, so the
/// whole stack is walked once to fix them and once to write them out.
struct Names {
    /// Card ids, by position.
    cards: Vec<String>,
    /// Card ids, by name.
    by_name: BTreeMap<String, String>,
    /// Part ids for the card being translated, then the background's, keyed
    /// by kind and lower-cased name.
    parts: BTreeMap<(String, String), String>,
    /// Every part on every card, for references that name the card they mean.
    everywhere: BTreeMap<(String, String, String), String>,
}

impl Elements for Names {
    fn id(&self, kind: &str, name: &str) -> Option<String> {
        self.parts
            .get(&(kind.to_string(), name.to_ascii_lowercase()))
            .cloned()
    }

    fn card_at(&self, position: usize) -> Option<String> {
        self.cards.get(position.checked_sub(1)?).cloned()
    }

    fn card_named(&self, name: &str) -> Option<String> {
        self.by_name.get(&name.to_ascii_lowercase()).cloned()
    }

    fn id_on_card(&self, card: &str, kind: &str, name: &str) -> Option<String> {
        let card = self.card_named(card)?;
        self.everywhere
            .get(&(card, kind.to_string(), name.to_ascii_lowercase()))
            .cloned()
    }
}

/// The id an element gets, decided once so that scripts and markup agree.
fn element_id(card: &str, layer: &str, name: &str) -> String {
    format!("{card}-{layer}-{}", slug(name))
}

/// Writes the stack as a page that runs in a browser.
///
/// The returned [`Translation::notes`] lists everything that had no equivalent,
/// once per occurrence, so a stack that came across whole says so.
#[must_use]
pub fn page(stack: &Stack) -> Translation {
    let Size { width, height } = stack.size();
    let mut notes = Vec::new();

    let cards: Vec<String> = (1..=stack.card_count())
        .map(|position| format!("hl-card-{position}"))
        .collect();
    let by_name: BTreeMap<String, String> = stack
        .cards()
        .iter()
        .zip(cards.iter())
        .filter(|(card, _)| !card.name().is_empty())
        .map(|(card, id)| (card.name().to_ascii_lowercase(), id.clone()))
        .collect();

    // Every id is worked out before anything is written, because a script on
    // one card may name a field on another and has to be able to find it.
    let mut everywhere = BTreeMap::new();
    for (index, card) in stack.cards().iter().enumerate() {
        let id = &cards[index];
        let background = stack.background_of(card.id());
        for (layer, part) in background
            .map(PartContainer::parts)
            .unwrap_or_default()
            .iter()
            .map(|part| ("bg", part))
            .chain(card.parts().iter().map(|part| ("card", part)))
        {
            everywhere.insert(
                (
                    id.clone(),
                    part.kind().as_str().to_string(),
                    part.name().to_ascii_lowercase(),
                ),
                element_id(id, layer, part.name()),
            );
        }
    }

    let mut body = String::new();
    for (index, card) in stack.cards().iter().enumerate() {
        let id = &cards[index];
        let background = stack.background_of(card.id());

        // The background's parts are shared by every card, so they are drawn
        // into each one. A name on the card wins over the same name on the
        // background, exactly as it does when the runtime looks one up.
        let mut names = Names {
            cards: cards.clone(),
            by_name: by_name.clone(),
            parts: BTreeMap::new(),
            everywhere: everywhere.clone(),
        };
        let mut drawn = Vec::new();
        for (layer, part) in background
            .map(PartContainer::parts)
            .unwrap_or_default()
            .iter()
            .map(|part| ("bg", part))
            .chain(card.parts().iter().map(|part| ("card", part)))
        {
            let element = element_id(id, layer, part.name());
            names.parts.insert(
                (
                    part.kind().as_str().to_string(),
                    part.name().to_ascii_lowercase(),
                ),
                element.clone(),
            );
            drawn.push((element, part));
        }

        let mut inside = String::new();
        for (element, part) in &drawn {
            inside.push_str(&self_part(element, part, &names, stack, &mut notes));
        }

        let card_script = handlers(card.script(), &names);
        notes.extend(card_script.notes);
        let attribute = if card_script.source.trim().is_empty() {
            String::new()
        } else {
            format!(" _=\"{}\"", escape(&card_script.source))
        };

        body.push_str(&format!(
            "<section class=\"hl-card\" id=\"{id}\" data-name=\"{}\"{attribute}>{inside}</section>\n",
            escape(card.name())
        ));
    }

    let stack_script = handlers(
        stack.script(),
        &Names {
            cards: cards.clone(),
            by_name,
            parts: BTreeMap::new(),
            everywhere,
        },
    );
    notes.extend(stack_script.notes);

    let source = document(stack.name(), width, height, &body, &stack_script.source);
    Translation { source, notes }
}

/// One part, as the element it becomes.
fn self_part(
    element: &str,
    part: &hyperlab_stack::Part,
    names: &Names,
    stack: &Stack,
    notes: &mut Vec<String>,
) -> String {
    let flag = |name: &str, fallback: bool| {
        part.property(name)
            .and_then(|value| value.as_bool())
            .unwrap_or(fallback)
    };
    let rect: Rect = part.geometry();
    let style = part
        .property("style")
        .unwrap_or(Value::Empty)
        .as_text()
        .to_ascii_lowercase();

    let mut css = format!(
        "left:{}px;top:{}px;width:{}px;height:{}px",
        rect.left, rect.top, rect.width, rect.height
    );
    if !flag("visible", true) {
        css.push_str(";display:none");
    }
    if style == "transparent" {
        css.push_str(";background:transparent;border:0");
    }

    let translated = handlers(part.script(), names);
    notes.extend(translated.notes.clone());
    let attribute = if translated.source.trim().is_empty() {
        String::new()
    } else {
        format!(" _=\"{}\"", escape(&translated.source))
    };

    let class = format!("hl-part hl-{}", part.kind().as_str());
    match part.part_kind() {
        PartKind::Button => {
            let label = if flag("showName", true) {
                escape(part.name())
            } else {
                String::new()
            };
            format!(
                "<button class=\"{class}\" id=\"{element}\" style=\"{css}\" \
                 aria-label=\"{}\"{attribute}>{label}</button>",
                escape(part.name())
            )
        }
        PartKind::Field => {
            let locked = if flag("locked", false) {
                " readonly"
            } else {
                ""
            };
            format!(
                "<textarea class=\"{class}\" id=\"{element}\" style=\"{css}\" \
                 aria-label=\"{}\"{locked}{attribute}>{}</textarea>",
                escape(part.name()),
                escape(&part.text())
            )
        }
        PartKind::Image => {
            let source = part.property("source").unwrap_or(Value::Empty).as_text();
            let Some(picture) = stack.image(&source) else {
                notes.push(format!("the picture \"{source}\" is not in the stack"));
                return String::new();
            };
            format!(
                "<img class=\"{class}\" id=\"{element}\" style=\"{css}\" alt=\"{}\" \
                 src=\"{}\"{attribute}>",
                escape(part.name()),
                escape(&hyperlab_stack::data_uri(picture))
            )
        }
    }
}

/// The page around the cards.
fn document(name: &str, width: i32, height: i32, body: &str, stack_script: &str) -> String {
    let stack_attribute = if stack_script.trim().is_empty() {
        String::new()
    } else {
        format!(" _=\"{}\"", escape(stack_script))
    };
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<script src="{LIBRARY}" integrity="{INTEGRITY}" crossorigin="anonymous"></script>
<style>
  body {{ margin: 0; padding: 24px; background: #d8d8d8;
         font: 13px/1.35 -apple-system, "Helvetica Neue", Helvetica, Arial, sans-serif; }}
  .hl-stack {{ position: relative; width: {width}px; height: {height}px; margin: 0 auto;
               background: #fff; border: 1px solid #000; overflow: hidden; }}
  .hl-card {{ position: absolute; inset: 0; display: none; }}
  .hl-card.hl-here {{ display: block; }}
  .hl-part {{ position: absolute; font: inherit; color: #000; overflow: hidden;
              box-sizing: border-box; }}
  .hl-button {{ background: #fff; border: 1px solid #000; border-radius: 9px;
                cursor: pointer; }}
  .hl-field {{ background: #fff; border: 1px solid #000; padding: 2px 4px;
               resize: none; }}
  .hl-image {{ border: 0; }}
  .hl-bar {{ width: {width}px; margin: 8px auto 0; display: flex; gap: 8px;
             align-items: center; }}
  .hl-bar button {{ font: inherit; }}
  .hl-bar span {{ margin-left: auto; color: #444; }}
</style>
</head>
<body{stack_attribute}>
<div class="hl-stack" id="hl-stack">
{body}</div>

<div class="hl-bar">
  <button _="on click call hlGo(hlCard - 1)">Previous</button>
  <button _="on click call hlGo(hlCard + 1)">Next</button>
  <span id="hl-where"></span>
</div>

<script>
// The part of a stack that is not a card: which one is showing, how to get to
// another, and the two messages HyperLab sends when you arrive and leave.
// Everything else on this page is the stack's own script, translated.
const hlCards = Array.from(document.querySelectorAll('.hl-card'));
let hlCard = 0;
const hlSeen = [];

// A translated script asking for `the number of cards` looks for `hlCards` on
// the global object, and `const` at the top of a script does not put it there
// — a function declaration does, which is why the helpers below need no such
// line and this does.
window.hlCards = hlCards;

function hlShow(next, remember) {{
  if (hlCards.length === 0) return;
  // Wraps at both ends, the way a stack of cards does.
  const at = ((next % hlCards.length) + hlCards.length) % hlCards.length;
  if (remember && at !== hlCard) hlSeen.push(hlCard);
  hlCards[hlCard].dispatchEvent(new CustomEvent('hyperlab:closecard'));
  hlCards.forEach((card, index) => card.classList.toggle('hl-here', index === at));
  hlCard = at;
  window.hlCard = at;
  document.getElementById('hl-where').textContent =
    `${{at + 1}} of ${{hlCards.length}} — ${{hlCards[at].dataset.name || ''}}`;
  hlCards[at].dispatchEvent(new CustomEvent('hyperlab:opencard'));
}}

// The three things HyperTalk does to text that _hyperscript has no words for.
// They live here rather than inline because a regular expression is a parse
// error inside a handler, and splitting words needs one.
const hlSplitOn = {{ char: '', word: /\s+/, item: ',', line: '\n' }};
const hlJoinWith = {{ char: '', word: ' ', item: ',', line: '\n' }};

function hlCount(text, kind) {{
  return kind === 'char' ? text.length : text.split(hlSplitOn[kind]).length;
}}

// `word 2 of x`, and `char 3 to 5 of x`. One-based and inclusive, as HyperTalk
// counts, which is why every index moves by one on the way in.
function hlPart(text, kind, from, to) {{
  if (kind === 'char') return text.slice(from - 1, to);
  return text.split(hlSplitOn[kind]).slice(from - 1, to).join(hlJoinWith[kind]);
}}

// Writing over one chunk of a piece of text: `put x into word 2 of y`.
// Splitting and rejoining is the same in every language and worth one
// function rather than four lines wherever a script does it.
function hlSplice(text, kind, from, to, value) {{
  if (kind === 'char') {{
    return text.slice(0, from - 1) + value + text.slice(to);
  }}
  const pieces = text.split(hlSplitOn[kind]);
  pieces.splice(from - 1, to - from + 1, value);
  return pieces.join(hlJoinWith[kind]);
}}

function hlGo(next) {{ hlShow(next, true); }}
function hlGoTo(id) {{ hlGo(hlCards.findIndex((card) => card.id === id)); }}
function hlBack() {{ if (hlSeen.length) hlShow(hlSeen.pop(), false); }}

hlShow(0, false);
</script>
</body>
</html>
"#,
        title = escape(name),
    )
}

#[cfg(test)]
mod tests {
    use hyperlab_stack::{PartKind, Rect, Stack};

    use super::*;

    fn with_button(script: &str) -> Stack {
        let mut stack = Stack::new("Test");
        let card = stack.cards()[0].id();
        let mut part = stack.new_part(PartKind::Button, "Press", Rect::new(10, 10, 80, 24));
        part.set_script(script);
        stack
            .card_mut(card)
            .expect("the card exists")
            .add_part(part);
        stack
    }

    #[test]
    fn a_stack_becomes_a_page_that_stands_on_its_own() {
        let written = page(&Stack::new("Empty")).source;
        assert!(written.starts_with("<!doctype html>"));
        assert!(written.contains("hyperscript.org@0.9.93"));
        assert!(written.contains("integrity=\"sha384-"));
        assert!(written.contains("<title>Empty</title>"));
    }

    #[test]
    fn a_buttons_script_rides_on_the_button() {
        let written = page(&with_button("on mouseUp\n  answer \"hi\"\nend mouseUp")).source;
        assert!(written.contains("<button"), "{written}");
        assert!(written.contains("on click call alert("), "{written}");
    }

    #[test]
    fn a_name_that_looks_like_markup_cannot_become_markup() {
        let mut stack = Stack::new("X");
        let card = stack.cards()[0].id();
        let part = stack.new_part(
            PartKind::Button,
            "<img onerror=alert(1)>",
            Rect::new(0, 0, 9, 9),
        );
        stack
            .card_mut(card)
            .expect("the card exists")
            .add_part(part);

        let written = page(&stack).source;
        assert!(!written.contains("<img onerror"), "a name became markup");
        assert!(written.contains("&lt;img onerror"), "{written}");
    }

    #[test]
    fn a_field_that_is_locked_says_so() {
        let mut stack = Stack::new("X");
        let card = stack.cards()[0].id();
        let mut part = stack.new_part(PartKind::Field, "Caption", Rect::new(0, 0, 90, 20));
        part.set_property("locked", true.into()).expect("ordinary");
        part.set_property("text", "hello".into()).expect("ordinary");
        stack
            .card_mut(card)
            .expect("the card exists")
            .add_part(part);

        let written = page(&stack).source;
        assert!(written.contains("readonly"), "{written}");
        assert!(written.contains(">hello</textarea>"), "{written}");
    }

    #[test]
    fn what_did_not_translate_is_counted() {
        let translated = page(&with_button("on mouseUp\n  beep\nend mouseUp"));
        assert_eq!(translated.notes.len(), 1);
        assert!(!translated.is_complete());
    }
}
