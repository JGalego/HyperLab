//! Exports every stack in `examples/` as a PDF into `docs/`.
//!
//! ```text
//! cargo run -p hyperlab-export --example export_examples
//! ```
//!
//! The PDFs sit beside the films of the same stacks and are named to match
//! them, so `docs/cluedo.gif` and `docs/cluedo.pdf` are the same stack moving
//! and standing still.
//!
//! Unlike `examples/` itself, these are **not** checked in CI, because the
//! same stack does not produce the same bytes twice. `svg2pdf` numbers the
//! objects in a converted picture in whatever order its hash map hands them
//! over, so two runs a second apart give files of identical size, with an
//! identical count of objects and streams, that differ from byte 20,413. And
//! words drawn inside a picture are set with whatever fonts the machine has,
//! so two machines differ again and for a second reason.
//!
//! Both are harmless — the documents are the same document — but they mean
//! rerunning this always shows a diff. Rerun it when a stack changes, not to
//! see whether anything did.

use std::path::{Path, PathBuf};

/// Each stack, and the name it goes out under.
///
/// The films got there first and the PDFs follow them, which is why the deck
/// is `deck` rather than what the stack calls itself.
const STACKS: [(&str, &str); 6] = [
    ("Address Book", "address-book"),
    ("Recipe Box", "recipe-box"),
    ("Todo", "todo"),
    ("Cluedo", "cluedo"),
    ("Myst", "myst"),
    ("Language Models, Explained", "deck"),
];

fn main() {
    let root = repository_root();
    let docs = root.join("docs");
    std::fs::create_dir_all(&docs).expect("the docs directory should be writable");

    for (stack, filename) in STACKS {
        let source = root.join("examples").join(format!("{stack}.hl"));
        let opened = hyperlab_persistence::load(&source)
            .unwrap_or_else(|error| panic!("could not open {stack}: {error}"));
        let pdf = hyperlab_export::to_pdf(&opened)
            .unwrap_or_else(|error| panic!("could not export {stack}: {error}"));

        let target = docs.join(format!("{filename}.pdf"));
        std::fs::write(&target, &pdf)
            .unwrap_or_else(|error| panic!("could not write {}: {error}", target.display()));
        let cards = opened.card_count();
        println!(
            "wrote {} — {cards} card{}, {} KB",
            target.display(),
            if cards == 1 { "" } else { "s" },
            pdf.len() / 1024
        );
    }
}

/// The repository root, found from where Cargo says this crate lives.
fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate is two levels below the repository root")
        .to_path_buf()
}
