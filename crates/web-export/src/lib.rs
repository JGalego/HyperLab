//! Turning a stack into a PDF or a Decker deck, in a browser.
//!
//! A module of its own, and the reason is weight. Drawing a picture into a
//! PDF page or a deck's bitmap needs an SVG renderer, a font database and a
//! text shaper; together they are most of what HyperLab compiles to in a
//! browser. Left in the main module they were paid for on every visit,
//! including by everyone who only wanted to click through Cluedo.
//!
//! So they live here, and the page fetches this module the first time
//! somebody actually exports something — the same bargain the typeface
//! already made.
//!
//! The seam is the stack's own single-file JSON, which
//! [`hyperlab_persistence`] already reads and writes. The main module hands
//! over the text; this one parses it and exports it. Nothing else crosses,
//! so the two modules share no state and cannot disagree about any.

#![warn(missing_docs)]

#[cfg(any(target_arch = "wasm32", test))]
use hyperlab_persistence::stack_from_single_file;
#[cfg(target_arch = "wasm32")]
use serde::Serialize;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// What a translation produced, and what it could not bring across.
///
/// Only the WebAssembly boundary has any use for it; the host build of this
/// crate exists to be tested, not to be called.
#[cfg(target_arch = "wasm32")]
#[derive(Serialize)]
struct Exported {
    source: String,
    notes: Vec<String>,
}

/// Reads the stack the page sent.
#[cfg(any(target_arch = "wasm32", test))]
fn parse(stack_json: &str) -> Result<hyperlab_stack::Stack, String> {
    stack_from_single_file(stack_json).map_err(|error| error.to_string())
}

// ------------------------------------------------------------------- wasm

/// Wakes the module up: a panic here should say what it was rather than
/// reaching the page as "unreachable".
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// Offers a typeface for the words drawn inside pictures.
///
/// A browser has no system fonts, so without this a picture's labels are
/// missing from whatever comes out. Both exporters are told.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn add_font(bytes: Vec<u8>) {
    hyperlab_export::add_font(bytes.clone());
    hyperlab_decker::add_font(bytes);
}

/// The stack as a PDF, one page per card.
///
/// Answers with the bytes themselves: a PDF is not text, and base64 through
/// a JSON channel would cost a third of the file for nothing.
///
/// # Errors
///
/// Returns the exporter's own message if the stack cannot be read or a
/// picture cannot be converted.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn to_pdf(stack_json: &str) -> Result<Vec<u8>, JsValue> {
    let stack = parse(stack_json).map_err(|error| JsValue::from_str(&error))?;
    hyperlab_export::to_pdf(&stack).map_err(|error| JsValue::from_str(&error.to_string()))
}

/// The stack as a Decker deck, with a line for everything Lil and a deck
/// have no equivalent for.
///
/// # Errors
///
/// Returns a message if the stack cannot be read.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn to_deck(stack_json: &str) -> Result<String, JsValue> {
    let stack = parse(stack_json).map_err(|error| JsValue::from_str(&error))?;
    let translated = hyperlab_decker::deck(&stack);
    serde_json::to_string(&Exported {
        source: translated.source,
        notes: translated.notes,
    })
    .map_err(|error| JsValue::from_str(&error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The seam this module is reached through, checked on the host: the
    /// text the main module sends must come back out as the same stack.
    #[test]
    fn a_stack_arrives_through_its_own_single_file_json() {
        let original = hyperlab_stack::Stack::new("Notes");
        let text = hyperlab_persistence::single_file_string(&original).unwrap();

        let parsed = parse(&text).expect("the text the other module sends");
        assert_eq!(parsed, original);

        // And the exporters can work with what came across.
        assert!(hyperlab_export::to_pdf(&parsed).is_ok());
        assert!(!hyperlab_decker::deck(&parsed).source.is_empty());
    }

    #[test]
    fn text_that_is_not_a_stack_is_refused_with_a_reason() {
        let error = parse("{ not json").expect_err("that is not a stack");
        assert!(
            error.contains("not valid HyperLab JSON"),
            "unhelpful: {error}"
        );
    }
}
