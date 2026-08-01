//! The HyperLab desktop shell.
//!
//! This crate contains no logic worth the name. It owns a [`Runtime`], turns
//! window events into runtime commands, and turns the runtime's state into a
//! snapshot the renderer can draw. Everything else lives in the core crates,
//! where it can be tested without a window.
//!
//! [`Runtime`]: hyperlab_runtime::Runtime

#![warn(missing_docs)]

mod ai_commands;
mod assistant;
mod commands;
mod dialogs;
mod settings;
mod state;
mod view;

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
            ai_commands::ai_view,
            ai_commands::ai_ask,
            ai_commands::ai_clear,
            ai_commands::ai_set_sends_field_text,
            ai_commands::ai_set_may_edit,
            ai_commands::ai_settings,
            ai_commands::ai_save_settings,
        ])
        .run(tauri::generate_context!())
        .expect("HyperLab could not open a window");
}
