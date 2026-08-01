//! HyperLab.

// Windows opens a console window for anything not marked as a GUI binary.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![warn(missing_docs)]

fn main() {
    hyperlab_desktop::run();
}
