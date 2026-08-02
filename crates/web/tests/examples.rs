//! The exact journey a stack takes to the playground, walked on the host.
//!
//! The website packs each example bundle as single-file JSON, ships it as a
//! static file, and the WebAssembly module reads it back and opens it. A
//! panic anywhere on that road is a blank card in a browser with no message
//! worth reading, so the same road is walked here, where a failure can
//! actually speak.

use hyperlab_persistence::{load, single_file_string, stack_from_single_file};
use hyperlab_runtime::Runtime;

#[test]
fn every_example_survives_the_trip_to_the_playground_and_opens() {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crates/web sits two levels below the root")
        .join("examples");

    let mut walked = 0;
    for entry in std::fs::read_dir(&examples).expect("examples/ should exist") {
        let path = entry.expect("readable directory").path();
        if path.extension().is_none_or(|kind| kind != "hl") {
            continue;
        }

        let stack =
            load(&path).unwrap_or_else(|error| panic!("{} should load: {error}", path.display()));
        let text = single_file_string(&stack).expect("a loaded stack serializes");
        let reread = stack_from_single_file(&text)
            .unwrap_or_else(|error| panic!("{} should reread: {error}", path.display()));
        assert_eq!(reread, stack, "{} changed in transit", path.display());

        let mut runtime = Runtime::new(reread);
        runtime
            .open_stack()
            .unwrap_or_else(|error| panic!("{} should open: {error}", path.display()));
        walked += 1;
    }
    assert!(walked >= 6, "only {walked} examples were walked");
}
