//! Packs the example stacks as single files, for the website.
//!
//! The bundles in `examples/` are the format to edit; the playground on the
//! website has no file system, so it is served each example as the one-file
//! JSON `save_single_file` writes. Run
//!
//! ```text
//! cargo run -p hyperlab-persistence --example pack_single_files -- <directory>
//! ```
//!
//! and every `examples/*.hl` bundle is loaded and rewritten there as
//! `<Name>.hl.json`. With no argument they go to `target/web-examples`.

use std::path::{Path, PathBuf};

use hyperlab_persistence::{BUNDLE_EXTENSION, load, single_file_string};

fn main() {
    let destination = std::env::args().nth(1).map_or_else(
        || repository_root().join("target/web-examples"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&destination).expect("the destination should be writable");

    let examples = repository_root().join("examples");
    let mut bundles: Vec<PathBuf> = std::fs::read_dir(&examples)
        .expect("examples/ should exist")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|kind| kind == BUNDLE_EXTENSION)
        })
        .collect();
    bundles.sort();

    assert!(
        !bundles.is_empty(),
        "no .hl bundles in {}",
        examples.display()
    );

    for bundle in bundles {
        let stack = load(&bundle)
            .unwrap_or_else(|error| panic!("{} should load: {error}", bundle.display()));
        let name = bundle
            .file_stem()
            .and_then(|name| name.to_str())
            .expect("a bundle has a name");
        let text = single_file_string(&stack).expect("a loaded stack serializes");
        let target = destination.join(format!("{name}.hl.json"));
        std::fs::write(&target, text)
            .unwrap_or_else(|error| panic!("could not write {}: {error}", target.display()));
        println!("packed {}", target.display());
    }
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/persistence sits two levels below the root")
        .to_path_buf()
}
