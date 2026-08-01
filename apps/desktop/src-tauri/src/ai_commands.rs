//! What the AI sidebar can ask the shell to do.
//!
//! Kept apart from [`commands`](crate::commands) because these are the only
//! commands that reach the network, and because a turn locks and unlocks the
//! session several times rather than holding it for the duration. Everything
//! that decides *when* is in [`AiState`].

use hyperlab_ai::AiSettings;
use tauri::State;

use crate::{
    assistant::{AiState, AiView},
    commands::{CommandResult, Outcome},
    settings,
    state::{AppState, lock},
};

/// Where the settings file lives.
fn directory(app: &tauri::AppHandle) -> CommandResult<std::path::PathBuf> {
    use tauri::Manager;
    app.path()
        .app_config_dir()
        .map_err(|error| format!("there is nowhere to keep settings: {error}"))
}

/// What the sidebar should draw.
#[tauri::command]
pub async fn ai_view(ai: State<'_, AiState>) -> CommandResult<AiView> {
    Ok(ai.view())
}

/// Asks the assistant something, and runs whatever it asks for.
///
/// Slow — it makes at least one network request — so it goes on a blocking
/// thread like every other command, and the window stays live throughout.
#[tauri::command]
pub async fn ai_ask(
    state: State<'_, AppState>,
    ai: State<'_, AiState>,
    question: String,
) -> CommandResult<Outcome> {
    if question.trim().is_empty() {
        return Err("there is nothing to ask".to_string());
    }

    let session = state.session();
    let assistant = ai.handle();

    let asked = tauri::async_runtime::spawn_blocking({
        let session = session.clone();
        let assistant = assistant.clone();
        move || assistant.ask(&session, &question)
    })
    .await
    .map_err(|_| "the assistant stopped unexpectedly".to_string())?;

    // The stack may have changed even if the turn failed part-way, so the
    // window is refreshed either way and the error is reported after.
    let outcome = {
        let mut held = lock(&session);
        crate::commands::snapshot_outcome(&mut held)
    };
    asked.map(|()| outcome)
}

/// Forgets the conversation.
#[tauri::command]
pub async fn ai_clear(ai: State<'_, AiState>) -> CommandResult<AiView> {
    ai.clear();
    Ok(ai.view())
}

/// Chooses whether the contents of fields are sent.
#[tauri::command]
pub async fn ai_set_sends_field_text(
    ai: State<'_, AiState>,
    sending: bool,
) -> CommandResult<AiView> {
    ai.set_sends_field_text(sending);
    Ok(ai.view())
}

/// Chooses whether the assistant may change the stack.
#[tauri::command]
pub async fn ai_set_may_edit(ai: State<'_, AiState>, editing: bool) -> CommandResult<AiView> {
    ai.set_may_edit(editing);
    Ok(ai.view())
}

/// The provider settings, for the settings panel.
#[tauri::command]
pub async fn ai_settings(ai: State<'_, AiState>) -> CommandResult<AiSettings> {
    Ok(ai.settings())
}

/// Writes new provider settings and rebuilds the providers.
#[tauri::command]
pub async fn ai_save_settings(
    app: tauri::AppHandle,
    ai: State<'_, AiState>,
    settings: AiSettings,
) -> CommandResult<AiView> {
    let directory = directory(&app)?;
    settings::save(&directory, &settings)?;

    let (registry, problems) = settings::build(&settings);
    ai.reconfigure(settings, registry, problems);
    Ok(ai.view())
}
