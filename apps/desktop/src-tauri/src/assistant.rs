//! The sidebar's half of a conversation.
//!
//! The turn loop lives in [`hyperlab_assistant`]; what is here is the part
//! that could not: holding the state between turns, and knowing when to let
//! go of the session lock.
//!
//! That second part is the whole reason this file exists. Asking a model
//! takes seconds and needs no stack; running a tool is instant and needs
//! `&mut Runtime`. Holding the lock across the request would stall every
//! other command — including [`dialog_reply`](crate::commands::dialog_reply),
//! which is how a script waiting on `ask` gets unstuck. So the lock is taken
//! for the tools, dropped for the network, and taken again.

use std::sync::{Arc, Mutex, PoisonError};

use hyperlab_ai::{AiSettings, ContextOptions, ProviderRegistry};
use hyperlab_assistant::{Briefing, Conversation, Entry, tools};
use hyperlab_mcp::{Access, Approval, Approver, Policy, ToolRegistry};
use serde::Serialize;

use crate::state::{Session, lock};

/// Everything the sidebar needs between turns.
///
/// Cheap to clone: every copy is the same conversation, which is what lets a
/// command hand one to a blocking thread.
#[derive(Clone)]
pub struct AiState {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    conversation: Conversation,
    registry: ProviderRegistry,
    settings: AiSettings,
    /// Why a provider named in the settings is not available.
    problems: Vec<String>,
    policy: Policy,
    context: ContextOptions,
    /// Whether a turn is already running. A second one would interleave tool
    /// calls with the first and corrupt both transcripts.
    busy: bool,
}

/// What the sidebar shows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiView {
    /// The conversation so far.
    pub entries: Vec<Entry>,
    /// The providers that are actually usable.
    pub providers: Vec<String>,
    /// Which one is being used.
    pub provider: Option<String>,
    /// Why any configured provider is missing.
    pub problems: Vec<String>,
    /// Whether field contents are being sent.
    pub sends_field_text: bool,
    /// Whether the assistant may change the stack.
    pub may_edit: bool,
    /// Whether a turn is in progress.
    pub busy: bool,
}

impl AiState {
    /// Reads the settings and builds whatever providers they describe.
    #[must_use]
    pub fn new(settings: AiSettings, registry: ProviderRegistry, problems: Vec<String>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                conversation: Conversation::new(),
                registry,
                settings,
                problems,
                // The user is sitting in front of the sidebar driving it, so
                // there is nobody to ask that they have not already told.
                policy: Policy::trusted(),
                context: ContextOptions::default(),
                busy: false,
            })),
        }
    }

    /// Another handle to the same conversation.
    #[must_use]
    pub fn handle(&self) -> Self {
        self.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// What the sidebar should draw.
    #[must_use]
    pub fn view(&self) -> AiView {
        let inner = self.lock();
        AiView {
            entries: inner.conversation.entries().to_vec(),
            providers: inner.registry.names().map(String::from).collect(),
            provider: inner.chosen(),
            problems: inner.problems.clone(),
            sends_field_text: inner.context.include_field_text,
            may_edit: !inner
                .policy
                .would_always_refuse("write_field", Access::Write),
            busy: inner.busy,
        }
    }

    /// Replaces the settings and rebuilds the providers.
    pub fn reconfigure(
        &self,
        settings: AiSettings,
        registry: ProviderRegistry,
        problems: Vec<String>,
    ) {
        let mut inner = self.lock();
        inner.settings = settings;
        inner.registry = registry;
        inner.problems = problems;
    }

    /// The settings as they stand.
    #[must_use]
    pub fn settings(&self) -> AiSettings {
        self.lock().settings.clone()
    }

    /// Whether to send the contents of fields.
    pub fn set_sends_field_text(&self, sending: bool) {
        let mut inner = self.lock();
        inner.context = if sending {
            ContextOptions::everything()
        } else {
            ContextOptions::default()
        };
    }

    /// Whether the assistant may change the stack.
    pub fn set_may_edit(&self, editing: bool) {
        let mut inner = self.lock();
        inner.policy = if editing {
            Policy::trusted()
        } else {
            Policy::new()
        };
    }

    /// Forgets the conversation.
    pub fn clear(&self) {
        self.lock().conversation.clear();
    }

    /// Answers one question in words, using no tools and sending no stack.
    ///
    /// This is what `ai("…")` and `ask assistant "…"` reach, and it is
    /// deliberately narrower than what the sidebar does, for two reasons.
    ///
    /// It cannot use tools: a script is *already inside* the runtime, which
    /// is locked and mid-handler. Letting an assistant restructure the stack
    /// between two statements would pull the ground out from under the
    /// interpreter. Edits belong in the sidebar, where nothing is running.
    ///
    /// It sends no description of the stack either, because it does not need
    /// to guess: the script says what to include. `ai("Summarize: " & field
    /// "Notes")` sends that field and nothing else.
    ///
    /// # Errors
    ///
    /// Returns a sentence for `the result` if no provider is set up or the
    /// provider could not be reached.
    pub fn answer(&self, prompt: &str) -> Result<String, String> {
        let (provider, model) = self.lock().provider()?;
        let request = hyperlab_ai::CompletionRequest::new(
            model,
            vec![
                hyperlab_ai::ChatMessage::system(hyperlab_assistant::SYSTEM_PROMPT),
                hyperlab_ai::ChatMessage::user(prompt),
            ],
        );
        tauri::async_runtime::block_on(provider.complete(request))
            .map(|completion| completion.content)
            .map_err(|error| error.to_string())
    }

    /// Takes a whole turn: ask, run whatever tools come back, ask again.
    ///
    /// # Errors
    ///
    /// Returns a sentence to show if no provider is set up, if one is already
    /// running, or if the provider could not be reached. Anything that goes
    /// wrong *within* a turn — a refused tool, a script that failed — is
    /// recorded in the conversation instead, because the assistant can act
    /// on those and the user should see them in order.
    pub fn ask(&self, session: &Arc<Mutex<Session>>, question: &str) -> Result<(), String> {
        let (provider, model) = {
            let mut inner = self.lock();
            if inner.busy {
                return Err("the assistant is still working on the last question".to_string());
            }
            let chosen = inner.provider()?;
            inner.busy = true;
            chosen
        };

        // Whatever happens from here, the sidebar must not be left stuck.
        let outcome = self.run_turn(session, question, &*provider, &model);
        self.lock().busy = false;

        if let Err(reason) = &outcome {
            self.lock().conversation.record_failure(reason.clone());
        }
        outcome
    }

    fn run_turn(
        &self,
        session: &Arc<Mutex<Session>>,
        question: &str,
        provider: &dyn hyperlab_ai::AiProvider,
        model: &str,
    ) -> Result<(), String> {
        let tools = ToolRegistry::new();

        {
            // Locked: describing the stack reads it.
            let held = lock(session);
            let briefing = Briefing::about(&held.runtime, self.lock().context);
            self.lock().conversation.ask(question, briefing);
        }

        loop {
            if !self.lock().conversation.begin_round() {
                return Err("the assistant kept using tools without answering".to_string());
            }

            let request = self.lock().conversation.request(model, tools.definitions());

            // Unlocked: this is the slow part, and it needs no stack.
            let completion = tauri::async_runtime::block_on(provider.complete(request))
                .map_err(|error| error.to_string())?;

            self.lock().conversation.record_reply(&completion);
            if completion.tool_calls.is_empty() {
                return Ok(());
            }

            // Locked again: tools go through the command bus like anyone else.
            let mut held = lock(session);
            let mut inner = self.lock();
            let Inner {
                policy,
                conversation,
                ..
            } = &mut *inner;
            let outcomes = tools::run(
                &mut held.runtime,
                &tools,
                policy,
                &mut SidebarUser,
                &completion.tool_calls,
            );
            let changed = outcomes.iter().any(|outcome| outcome.allowed);
            for outcome in &outcomes {
                conversation.record_tool(outcome);
            }
            if changed {
                held.touch();
            }
        }
    }
}

impl Inner {
    fn chosen(&self) -> Option<String> {
        self.settings
            .default_provider
            .clone()
            .filter(|name| self.registry.get(name).is_some())
            .or_else(|| self.registry.names().next().map(String::from))
    }

    /// The provider to use, and the model to ask it for.
    fn provider(&self) -> Result<(Arc<dyn hyperlab_ai::AiProvider>, String), String> {
        let name = self
            .chosen()
            .ok_or("no language model is set up yet — add one in AI ▸ Settings")?;
        let provider = self
            .registry
            .get(&name)
            .ok_or_else(|| format!("the provider \"{name}\" is no longer available"))?;
        let model = self
            .settings
            .providers
            .get(&name)
            .map(|config| config.model.clone())
            .unwrap_or_default();
        Ok((provider, model))
    }
}

/// The person at the keyboard, who is already driving the sidebar.
///
/// They opened it, typed a question and pressed Return, and the panel says
/// whether the assistant may make changes. Stopping to ask again per tool
/// would be a dialog every few seconds saying what the switch already said.
struct SidebarUser;

impl Approver for SidebarUser {
    fn approve(&mut self, _request: &Approval<'_>) -> bool {
        true
    }
}
