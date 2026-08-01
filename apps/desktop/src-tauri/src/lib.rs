//! The HyperLab desktop shell.
//!
//! This crate contains no logic worth the name. It owns a [`Runtime`], turns
//! window events into runtime commands, and turns the runtime's state into a
//! snapshot the renderer can draw. Everything else lives in the core crates,
//! where it can be tested without a window.
//!
//! [`Runtime`]: hyperlab_runtime::Runtime

#![warn(missing_docs)]

mod commands;
mod state;
mod view;

pub use state::AppState;

/// Starts the application.
///
/// # Panics
///
/// Panics if the window cannot be created, which means the platform's web
/// view is missing — there is nothing sensible to do but say so.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::get_view,
            commands::get_properties,
            commands::check_script,
            commands::click_part,
            commands::set_field_text,
            commands::go_to_card,
            commands::run_message_box,
            commands::new_card,
            commands::delete_card,
            commands::new_part,
            commands::delete_part,
            commands::set_geometry,
            commands::set_property,
            commands::set_script,
            commands::rename,
            commands::set_stack_size,
            commands::undo,
            commands::redo,
            commands::new_stack,
            commands::open_stack,
            commands::save_stack,
            commands::part_at,
        ])
        .run(tauri::generate_context!())
        .expect("HyperLab could not open a window");
}
