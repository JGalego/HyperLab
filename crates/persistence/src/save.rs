//! Writing a stack to disk.

use std::{
    fs,
    path::{Path, PathBuf},
};

use hyperlab_stack::{
    Background, Card, Object, ObjectKind, Part, PartContainer, Stack, now_millis,
};

use crate::{
    error::{PersistenceError, PersistenceResult},
    format::{FORMAT_VERSION, Metadata, StackDocument, script_file_name},
};

/// Writes a stack to a `.hl` bundle, creating it if necessary.
///
/// The write replaces the bundle's contents: cards, backgrounds and scripts
/// that are no longer in the stack are removed, so a saved bundle always
/// matches the stack exactly.
///
/// # Errors
///
/// Returns a [`PersistenceError`] if anything cannot be written.
pub fn save(path: impl AsRef<Path>, stack: &Stack) -> PersistenceResult<()> {
    let root = path.as_ref();
    create_dir(root)?;
    // Clearing first is what makes a save a replacement rather than a merge:
    // a card, script or picture that is no longer in the stack must not
    // survive in the bundle.
    for directory in ["cards", "backgrounds", "scripts", "images"] {
        remove_dir(&root.join(directory))?;
        create_dir(&root.join(directory))?;
    }

    for (name, image) in stack.images() {
        // The name was checked when the picture entered the model, so it is
        // a file name and cannot climb out of the bundle. Checking again
        // here costs nothing and means a hand-built `Stack` cannot either.
        let path = root.join("images").join(safe_file_name(name)?);
        fs::write(&path, image.bytes()).map_err(|error| PersistenceError::io(path, error))?;
    }

    for background in stack.backgrounds() {
        let (stripped, scripts) = strip_background(background);
        write_scripts(root, &scripts)?;
        write_json(
            &root
                .join("backgrounds")
                .join(format!("{}.json", background.id())),
            &stripped,
        )?;
    }

    for card in stack.cards() {
        let (stripped, scripts) = strip_card(card);
        write_scripts(root, &scripts)?;
        write_json(
            &root.join("cards").join(format!("{}.json", card.id())),
            &stripped,
        )?;
    }

    write_scripts(
        root,
        &[(
            script_file_name(ObjectKind::Stack, stack.id()),
            stack.script().to_string(),
        )],
    )?;

    let document = StackDocument {
        id: stack.id().get(),
        name: stack.name().to_string(),
        size: stack.size(),
        properties: stack.core().properties.clone(),
        next_id: stack.peek_next_id().get(),
        created_at: stack.core().created_at,
        updated_at: stack.core().updated_at,
        backgrounds: stack
            .backgrounds()
            .iter()
            .map(|background| background.id().get())
            .collect(),
        cards: stack.cards().iter().map(|card| card.id().get()).collect(),
    };
    write_json(&root.join("stack.json"), &document)?;

    let metadata = Metadata {
        format_version: FORMAT_VERSION,
        name: stack.name().to_string(),
        saved_at: now_millis(),
        card_count: stack.card_count(),
    };
    write_json(&root.join("metadata.json"), &metadata)?;
    Ok(())
}

/// Writes a stack as one self-contained JSON file.
///
/// The bundle is the format to edit; this is the format to email. It holds
/// exactly the same information.
///
/// # Errors
///
/// Returns a [`PersistenceError`] if the stack cannot be written.
pub fn save_single_file(path: impl AsRef<Path>, stack: &Stack) -> PersistenceResult<()> {
    write_json(path.as_ref(), stack)
}

/// Renders a stack as the same self-contained JSON document
/// [`save_single_file`] writes, for a caller with no file system — a browser,
/// a clipboard, a wire.
///
/// # Errors
///
/// Returns a [`PersistenceError`] if the stack cannot be serialized, which a
/// well-formed [`Stack`] never is.
pub fn single_file_string(stack: &Stack) -> PersistenceResult<String> {
    serde_json::to_string_pretty(stack)
        .map(|json| json + "\n")
        .map_err(|error| PersistenceError::json("a single-file stack", error))
}

/// Refuses a name that is not a plain file name.
///
/// A picture's name reaches this function from a `Stack`, and a `Stack` can
/// be built in memory by anything — a hand-edited file, an MCP client, a
/// future importer. `Image::new` already refuses a path, so this is the
/// second of two locks on the same door, and the one standing closest to
/// the filesystem call.
fn safe_file_name(name: &str) -> PersistenceResult<&str> {
    let sane = !name.is_empty()
        && !name.starts_with('.')
        && !name.contains('/')
        && !name.contains('\\')
        && Path::new(name).file_name().is_some_and(|only| only == name);
    if sane {
        Ok(name)
    } else {
        Err(PersistenceError::Incomplete(format!(
            "\"{name}\" is not a file name, so it cannot be written into the bundle"
        )))
    }
}

/// A file name and the script that goes in it.
type ScriptFile = (String, String);

/// Takes the scripts out of a card, returning a copy with empty scripts.
fn strip_card(card: &Card) -> (Card, Vec<ScriptFile>) {
    let mut copy = card.clone();
    let mut scripts = vec![(
        script_file_name(ObjectKind::Card, card.id()),
        card.script().to_string(),
    )];
    scripts.extend(strip_parts(copy.parts_mut()));
    copy.core_mut().script.clear();
    (copy, scripts)
}

/// Takes the scripts out of a background, returning a copy with empty scripts.
fn strip_background(background: &Background) -> (Background, Vec<ScriptFile>) {
    let mut copy = background.clone();
    let mut scripts = vec![(
        script_file_name(ObjectKind::Background, background.id()),
        background.script().to_string(),
    )];
    scripts.extend(strip_parts(copy.parts_mut()));
    copy.core_mut().script.clear();
    (copy, scripts)
}

fn strip_parts(parts: &mut [Part]) -> Vec<ScriptFile> {
    parts
        .iter_mut()
        .map(|part| {
            let name = script_file_name(part.kind(), part.id());
            let source = std::mem::take(&mut part.core_mut().script);
            (name, source)
        })
        .collect()
}

/// Writes the non-empty scripts. An object with no script gets no file,
/// which keeps `scripts/` a list of the code that actually exists.
fn write_scripts(root: &Path, scripts: &[ScriptFile]) -> PersistenceResult<()> {
    for (name, source) in scripts {
        if source.trim().is_empty() {
            continue;
        }
        let path = root.join("scripts").join(name);
        let mut text = source.clone();
        if !text.ends_with('\n') {
            text.push('\n');
        }
        fs::write(&path, text).map_err(|error| PersistenceError::io(path, error))?;
    }
    Ok(())
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> PersistenceResult<()> {
    // Pretty-printed, because these files are meant to be read and diffed.
    let json =
        serde_json::to_string_pretty(value).map_err(|error| PersistenceError::json(path, error))?;
    fs::write(path, json + "\n").map_err(|error| PersistenceError::io(path, error))
}

fn create_dir(path: &Path) -> PersistenceResult<()> {
    fs::create_dir_all(path).map_err(|error| PersistenceError::io(path, error))
}

fn remove_dir(path: &PathBuf) -> PersistenceResult<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(PersistenceError::io(path, error)),
    }
}
