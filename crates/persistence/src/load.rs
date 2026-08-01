//! Reading a stack from disk.

use std::{fs, path::Path};

use hyperlab_stack::{Background, Card, Id, Object, ObjectKind, PartContainer, Stack};

use crate::{
    error::{PersistenceError, PersistenceResult},
    format::{FORMAT_VERSION, Metadata, StackDocument, script_file_name},
    migrate,
};

/// Reads a stack from a `.hl` bundle.
///
/// # Errors
///
/// Returns a [`PersistenceError`] if the bundle is missing, unreadable,
/// written by a newer version of HyperLab, or incomplete.
pub fn load(path: impl AsRef<Path>) -> PersistenceResult<Stack> {
    let root = path.as_ref();
    let metadata: Metadata = read_json(&root.join("metadata.json"))?;
    migrate::check_version(metadata.format_version, FORMAT_VERSION)?;

    let document: StackDocument = read_json(&root.join("stack.json"))?;
    let mut stack = Stack::empty(Id::new(document.id), document.name);
    stack.set_size(document.size);
    for (name, value) in &document.properties {
        // `set_property` rejects read-only names, which a hand-edited file
        // might contain; ignoring them is kinder than refusing to open.
        let _ = stack.set_property(name, value.clone());
    }

    for id in &document.backgrounds {
        let id = Id::new(*id);
        let mut background: Background =
            read_json(&root.join("backgrounds").join(format!("{id}.json")))?;
        attach_scripts(root, &mut background)?;
        stack.insert_background(background);
    }

    for id in &document.cards {
        let id = Id::new(*id);
        let mut card: Card = read_json(&root.join("cards").join(format!("{id}.json")))?;
        attach_scripts(root, &mut card)?;
        if stack.background(card.background()).is_none() {
            return Err(PersistenceError::Incomplete(format!(
                "card {id} sits on background {}, which is not in this bundle",
                card.background()
            )));
        }
        stack.add_card(card);
    }

    if let Some(source) = read_script(root, ObjectKind::Stack, stack.id())? {
        stack.set_script(&source);
    }

    // Ids are never reused, so start where the last session left off.
    stack.reserve_id(Id::new(document.next_id.saturating_sub(1)));

    // Timestamps are restored last: everything above counts as a change.
    stack.core_mut().created_at = document.created_at;
    stack.core_mut().updated_at = document.updated_at;

    if stack.is_empty() {
        return Err(PersistenceError::Incomplete(
            "it contains no cards at all".to_string(),
        ));
    }
    Ok(stack)
}

/// Reads a stack from a single JSON file, as written by
/// [`save_single_file`](crate::save_single_file).
///
/// # Errors
///
/// Returns a [`PersistenceError`] if the file cannot be read or parsed.
pub fn load_single_file(path: impl AsRef<Path>) -> PersistenceResult<Stack> {
    let stack: Stack = read_json(path.as_ref())?;
    if stack.is_empty() {
        return Err(PersistenceError::Incomplete(
            "it contains no cards at all".to_string(),
        ));
    }
    Ok(stack)
}

/// Reads a bundle's metadata without loading the stack, for file browsers.
///
/// # Errors
///
/// Returns a [`PersistenceError`] if the metadata cannot be read.
pub fn read_metadata(path: impl AsRef<Path>) -> PersistenceResult<Metadata> {
    read_json(&path.as_ref().join("metadata.json"))
}

/// Puts the scripts back on a card or background and its parts, and brings
/// any part saved by an older version up to date.
fn attach_scripts<T>(root: &Path, container: &mut T) -> PersistenceResult<()>
where
    T: Object + PartContainer,
{
    if let Some(source) = read_script(root, container.kind(), container.id())? {
        set_script_without_touching(container, &source);
    }
    for part in container.parts_mut() {
        // Fills in properties this version expects and older files lack.
        part.apply_defaults();
        if let Some(source) = read_script(root, part.kind(), part.id())? {
            set_script_without_touching(part, &source);
        }
    }
    Ok(())
}

/// Puts a script back without making the object look freshly edited.
///
/// Opening a stack is not a change to it: an object loaded today must report
/// the same `updated_at` it had when it was saved.
fn set_script_without_touching(object: &mut impl Object, source: &str) {
    let updated_at = object.core().updated_at;
    object.set_script(source);
    object.core_mut().updated_at = updated_at;
}

/// Reads an object's script file, if it has one.
fn read_script(root: &Path, kind: ObjectKind, id: Id) -> PersistenceResult<Option<String>> {
    let path = root.join("scripts").join(script_file_name(kind, id));
    match fs::read_to_string(&path) {
        Ok(source) => Ok(Some(source.trim_end().to_string())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(PersistenceError::io(path, error)),
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> PersistenceResult<T> {
    let text = fs::read_to_string(path).map_err(|error| PersistenceError::io(path, error))?;
    serde_json::from_str(&text).map_err(|error| PersistenceError::json(path, error))
}
