//! Saving and loading, including the awkward cases.

use std::{fs, path::PathBuf};

use hyperlab_persistence::{
    PersistenceError, load, load_single_file, read_metadata, save, save_single_file,
    single_file_string, stack_from_single_file,
};
use hyperlab_stack::{Image, Object, PartContainer, PartKind, Rect, Size, Stack, Value};

/// A temporary directory that cleans up after itself, so the tests leave
/// nothing behind even when they fail.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let base = std::env::temp_dir().join(format!(
            "hyperlab-test-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::remove_dir_all(&base).ok();
        fs::create_dir_all(&base).unwrap();
        Self(base)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

/// A stack with something of everything in it.
fn sample() -> Stack {
    let mut stack = Stack::new("Address Book");
    stack.set_size(Size::new(400, 300));
    stack.set_script("on openStack\n  go to first card\nend openStack");
    stack.set_property("author", Value::text("Ada")).unwrap();

    let card_id = stack.cards()[0].id();
    let background_id = stack.backgrounds()[0].id();

    let mut button = stack.new_part(PartKind::Button, "Next", Rect::new(10, 20, 80, 24));
    button.set_script("on mouseUp\n  go to next card\nend mouseUp");
    stack
        .background_mut(background_id)
        .unwrap()
        .add_part(button);

    let mut field = stack.new_part(PartKind::Field, "Name", Rect::new(10, 60, 200, 22));
    field
        .set_property("text", Value::text("Ada Lovelace"))
        .unwrap();
    stack.card_mut(card_id).unwrap().add_part(field);

    let second = stack.new_card(background_id).unwrap();
    stack.add_card(second);
    stack
}

#[test]
fn a_stack_survives_a_bundle_round_trip() {
    let temp = TempDir::new("round-trip");
    let path = temp.path("Sample.hl");
    let original = sample();

    save(&path, &original).unwrap();
    let reloaded = load(&path).unwrap();

    assert_eq!(reloaded, original, "the bundle must be lossless");
}

#[test]
fn the_bundle_has_the_documented_shape() {
    let temp = TempDir::new("shape");
    let path = temp.path("Sample.hl");
    save(&path, &sample()).unwrap();

    for expected in [
        "metadata.json",
        "stack.json",
        "cards",
        "backgrounds",
        "scripts",
        "images",
    ] {
        assert!(
            path.join(expected).exists(),
            "the bundle is missing {expected}"
        );
    }
    assert_eq!(
        fs::read_dir(path.join("cards")).unwrap().count(),
        2,
        "one file per card"
    );
}

#[test]
fn scripts_are_stored_as_readable_files() {
    let temp = TempDir::new("scripts");
    let path = temp.path("Sample.hl");
    save(&path, &sample()).unwrap();

    let scripts: Vec<String> = fs::read_dir(path.join("scripts"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        scripts.len(),
        2,
        "only the stack and the button have scripts, got {scripts:?}"
    );

    let button_script = scripts
        .iter()
        .find(|name| name.starts_with("button-"))
        .expect("the button's script should have its own file");
    let source = fs::read_to_string(path.join("scripts").join(button_script)).unwrap();
    assert!(
        source.starts_with("on mouseUp"),
        "scripts are stored as plain HyperTalk, not escaped JSON"
    );
}

#[test]
fn metadata_can_be_read_without_loading_the_stack() {
    let temp = TempDir::new("metadata");
    let path = temp.path("Sample.hl");
    save(&path, &sample()).unwrap();

    let metadata = read_metadata(&path).unwrap();
    assert_eq!(metadata.name, "Address Book");
    assert_eq!(metadata.card_count, 2);
    assert_eq!(
        metadata.format_version,
        hyperlab_persistence::FORMAT_VERSION
    );
}

#[test]
fn saving_again_removes_what_is_no_longer_there() {
    let temp = TempDir::new("replace");
    let path = temp.path("Sample.hl");
    let mut stack = sample();
    save(&path, &stack).unwrap();
    assert_eq!(fs::read_dir(path.join("cards")).unwrap().count(), 2);

    let second = stack.cards()[1].id();
    stack.remove_card(second).unwrap();
    save(&path, &stack).unwrap();

    assert_eq!(
        fs::read_dir(path.join("cards")).unwrap().count(),
        1,
        "the deleted card's file must go too"
    );
    assert_eq!(load(&path).unwrap().card_count(), 1);
}

#[test]
fn ids_are_never_reused_after_a_reload() {
    let temp = TempDir::new("ids");
    let path = temp.path("Sample.hl");
    let mut stack = sample();
    let before = stack.next_id();
    save(&path, &stack).unwrap();

    let mut reloaded = load(&path).unwrap();
    assert!(
        reloaded.next_id().get() > before.get(),
        "a reloaded stack must not hand out an id it has already used"
    );
}

#[test]
fn a_single_file_holds_the_same_stack() {
    let temp = TempDir::new("single");
    let path = temp.path("Sample.json");
    let original = sample();

    save_single_file(&path, &original).unwrap();
    assert_eq!(load_single_file(&path).unwrap(), original);
}

#[test]
fn a_single_file_string_holds_the_same_stack_with_no_file_at_all() {
    let original = sample();
    let text = single_file_string(&original).unwrap();
    assert_eq!(stack_from_single_file(&text).unwrap(), original);
}

#[test]
fn the_string_is_byte_for_byte_what_the_file_would_hold() {
    let temp = TempDir::new("single-string");
    let path = temp.path("Sample.json");
    let original = sample();

    save_single_file(&path, &original).unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        single_file_string(&original).unwrap()
    );
}

#[test]
fn a_single_file_with_no_cards_is_refused() {
    // Hand-built JSON, because no honest save can produce a cardless stack.
    let text = single_file_string(&sample()).unwrap();
    let mut broken: serde_json::Value = serde_json::from_str(&text).unwrap();
    broken["cards"] = serde_json::json!([]);
    match stack_from_single_file(&broken.to_string()) {
        Err(PersistenceError::Incomplete(reason)) => assert!(reason.contains("no cards")),
        other => panic!("expected an incompleteness error, got {other:?}"),
    }
}

#[test]
fn text_that_is_not_a_stack_is_refused_readably() {
    let error = stack_from_single_file("{ not json").unwrap_err();
    assert!(
        error.to_string().contains("not valid HyperLab JSON"),
        "unhelpful: {error}"
    );
}

#[test]
fn a_bundle_from_the_future_is_refused_clearly() {
    let temp = TempDir::new("future");
    let path = temp.path("Sample.hl");
    save(&path, &sample()).unwrap();

    let metadata_path = path.join("metadata.json");
    let text = fs::read_to_string(&metadata_path).unwrap();
    fs::write(
        &metadata_path,
        text.replace("\"formatVersion\": 1", "\"formatVersion\": 99"),
    )
    .unwrap();

    match load(&path) {
        Err(PersistenceError::UnsupportedVersion { found, .. }) => assert_eq!(found, 99),
        other => panic!("expected a version error, got {other:?}"),
    }
}

#[test]
fn a_missing_bundle_names_the_file_it_wanted() {
    let temp = TempDir::new("missing");
    let error = load(temp.path("Nowhere.hl")).unwrap_err();
    assert!(
        error.to_string().contains("metadata.json"),
        "unhelpful error: {error}"
    );
}

#[test]
fn a_card_without_its_background_is_reported_rather_than_loaded() {
    let temp = TempDir::new("orphan");
    let path = temp.path("Sample.hl");
    let stack = sample();
    save(&path, &stack).unwrap();

    let background = stack.backgrounds()[0].id();
    fs::remove_file(path.join("backgrounds").join(format!("{background}.json"))).unwrap();
    // Keep stack.json honest about what is left.
    let stack_json = path.join("stack.json");
    let text = fs::read_to_string(&stack_json).unwrap();
    let patched = text.replace(&format!("{background}"), "");
    fs::write(
        &stack_json,
        patched.replace("\"backgrounds\": [\n    \n  ]", "\"backgrounds\": []"),
    )
    .unwrap();

    let error = load(&path).unwrap_err();
    assert!(
        matches!(error, PersistenceError::Incomplete(_)),
        "expected an incomplete-bundle error, got {error}"
    );
}

#[test]
fn parts_saved_without_todays_properties_are_brought_up_to_date() {
    let temp = TempDir::new("upgrade");
    let path = temp.path("Sample.hl");
    save(&path, &sample()).unwrap();

    // Simulate a bundle written before `enabled` existed.
    let cards = path.join("cards");
    let card_file = fs::read_dir(&cards)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let text = fs::read_to_string(&card_file).unwrap();
    fs::write(&card_file, text.replace("\"enabled\": true,", "")).unwrap();

    let reloaded = load(&path).unwrap();
    let card = reloaded
        .cards()
        .iter()
        .find(|card| !card.parts().is_empty())
        .expect("one card has a field");
    assert_eq!(
        card.parts()[0].property("enabled"),
        Some(Value::Bool(true)),
        "loading fills in properties that older files lack"
    );
}

// ------------------------------------------------------------------ pictures

/// The smallest real PNG: one transparent pixel.
const PIXEL: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

#[test]
fn pictures_are_written_as_pictures_and_come_back() {
    let mut stack = sample();
    stack.set_image(
        "dot.png",
        Some(Image::new("dot.png", PIXEL.to_vec()).unwrap()),
    );
    stack.set_image(
        "mark.svg",
        Some(Image::new("mark.svg", b"<svg viewBox=\"0 0 1 1\"/>".to_vec()).unwrap()),
    );

    let temp = TempDir::new("pictures");
    let path = temp.path("Sample.hl");
    save(&path, &stack).unwrap();

    // A real file, byte for byte, that any image viewer opens.
    assert_eq!(fs::read(path.join("images/dot.png")).unwrap(), PIXEL);
    assert!(
        fs::read_to_string(path.join("images/mark.svg"))
            .unwrap()
            .starts_with("<svg"),
        "an SVG should stay text"
    );

    let loaded = load(&path).unwrap();
    assert_eq!(loaded.images().len(), 2);
    assert_eq!(loaded.image("dot.png").unwrap().bytes(), PIXEL);
}

#[test]
fn a_picture_removed_from_the_stack_leaves_the_bundle() {
    let mut stack = sample();
    stack.set_image(
        "dot.png",
        Some(Image::new("dot.png", PIXEL.to_vec()).unwrap()),
    );
    let temp = TempDir::new("removed");
    let path = temp.path("Sample.hl");
    save(&path, &stack).unwrap();

    stack.set_image("dot.png", None);
    save(&path, &stack).unwrap();

    assert!(
        !path.join("images/dot.png").exists(),
        "a save is a replacement, not a merge"
    );
    assert!(load(&path).unwrap().images().is_empty());
}

#[test]
fn a_bundle_from_before_pictures_still_opens() {
    let temp = TempDir::new("no-images");
    let path = temp.path("Sample.hl");
    save(&path, &sample()).unwrap();
    fs::remove_dir_all(path.join("images")).unwrap();

    let loaded = load(&path).unwrap();
    assert!(loaded.images().is_empty());
}

#[test]
fn a_picture_smuggled_into_the_bundle_by_hand_is_refused() {
    let temp = TempDir::new("smuggled");
    let path = temp.path("Sample.hl");
    save(&path, &sample()).unwrap();
    // Named like a picture, and is not one. Opening must say so rather than
    // hand a web view something it did not expect.
    fs::write(path.join("images/evil.png"), b"<script>alert(1)</script>").unwrap();

    let error = load(&path).unwrap_err();
    assert!(
        matches!(error, PersistenceError::Incomplete(ref why) if why.contains("evil.png")),
        "got {error}"
    );
}

#[test]
fn a_stack_written_by_hand_cannot_write_outside_the_bundle() {
    // `Image::new` refuses a path, so getting one into a `Stack` means going
    // around it — a hand-edited file, or a future importer. The saver must
    // refuse too, because it is the code holding the pen.
    let mut stack = sample();
    stack.set_image(
        "../escaped.png",
        Some(Image::new("dot.png", PIXEL.to_vec()).unwrap()),
    );
    let temp = TempDir::new("escape");
    let path = temp.path("Sample.hl");

    assert!(save(&path, &stack).is_err());
    assert!(!temp.path("escaped.png").exists());
}
