//! The HyperLab desktop shell.
//!
//! This crate contains no logic worth the name. It owns a [`Runtime`], turns
//! window events into runtime commands, and turns the runtime's state into a
//! snapshot the renderer can draw. Everything else lives in the core crates,
//! where it can be tested without a window.
//!
//! [`Runtime`]: hyperlab_runtime::Runtime

#![warn(missing_docs)]

// Public so that the development bridge in `src/bin/` can drive the same
// runtime, host and snapshot the window does, rather than a second copy of
// them. This is an application, not a library: there is no API here for
// anyone else to depend on.
mod ai_commands;
pub mod assistant;
mod commands;
pub mod dialogs;
pub mod keys;
pub mod settings;
pub mod state;
pub mod view;

use std::sync::Arc;

use tauri::{Manager, WindowEvent};

pub use assistant::{AiState, AiView};
pub use dialogs::{DIALOG_EVENT, DesktopHost, DialogRequest, Dialogs};
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
        .setup(|app| {
            // Settings can only be read now: where they live is something
            // only a running application knows.
            let (ai_settings, problems) = match app.path().app_config_dir() {
                Ok(directory) => match settings::load(&directory) {
                    Ok(settings) => (settings, Vec::new()),
                    // A broken settings file must not stop the application
                    // opening; the sidebar shows why instead.
                    Err(reason) => (hyperlab_ai::AiSettings::default(), vec![reason]),
                },
                Err(error) => (
                    hyperlab_ai::AiSettings::default(),
                    vec![format!("there is nowhere to keep settings: {error}")],
                ),
            };
            let (registry, mut trouble) = settings::build(&ai_settings);
            trouble.extend(problems);
            let assistant = AiState::new(ai_settings, registry, trouble);
            app.manage(assistant.handle());

            // The host needs a window to show dialogs on, so it can only be
            // built now.
            let state = app.state::<AppState>();
            let host = DesktopHost::new(app.handle().clone(), state.dialogs(), assistant);
            state.install_host(Box::new(host));
            Ok(())
        })
        .on_window_event(|window, event| {
            // A script waiting for an answer that can no longer arrive would
            // hold the runtime open for ever.
            if matches!(
                event,
                WindowEvent::Destroyed | WindowEvent::CloseRequested { .. }
            ) {
                let dialogs: Arc<Dialogs> = window.state::<AppState>().dialogs();
                dialogs.cancel();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::dialog_reply,
            commands::get_view,
            commands::get_properties,
            commands::check_script,
            commands::stack_graph,
            commands::stack_image,
            commands::stack_images,
            commands::import_image,
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
            commands::export_pdf,
            commands::export_web,
            commands::export_png,
            commands::save_stack,
            commands::part_at,
            ai_commands::ai_view,
            ai_commands::ai_ask,
            ai_commands::ai_clear,
            ai_commands::ai_set_sends_field_text,
            ai_commands::ai_set_may_edit,
            ai_commands::ai_settings,
            ai_commands::ai_save_settings,
            ai_commands::ai_keychain,
            ai_commands::ai_set_key,
            ai_commands::ai_forget_key,
        ])
        .run(tauri::generate_context!())
        .expect("HyperLab could not open a window");
}

#[cfg(test)]
mod tests {
    /// The window's policy, read from the file the bundler reads.
    const CONFIG: &str = include_str!("../tauri.conf.json");

    /// The renderer draws every picture as a `data:` URI, and the window
    /// applies a Content Security Policy that the development server does
    /// not. Those two facts met for the first time in a release build, where
    /// every picture in every stack silently failed to load.
    #[test]
    fn the_window_policy_allows_the_pictures_we_draw() {
        let config: serde_json::Value =
            serde_json::from_str(CONFIG).expect("tauri.conf.json should be valid JSON");
        let policy = config["app"]["security"]["csp"]
            .as_str()
            .expect("there should be a csp");

        let images = policy
            .split(';')
            .map(str::trim)
            .find(|directive| directive.starts_with("img-src"))
            .unwrap_or_else(|| {
                panic!("no img-src in \"{policy}\", so default-src decides and blocks data: URIs")
            });
        assert!(
            images.contains("data:"),
            "img-src must allow data:, or no picture in any stack will draw: \"{images}\""
        );
    }
}
