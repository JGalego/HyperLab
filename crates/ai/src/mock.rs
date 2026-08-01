//! A provider that needs no network.

use std::sync::Mutex;

use crate::{
    message::{ChatMessage, Completion, CompletionRequest, Embedding, FinishReason, Role},
    provider::{AiProvider, AiResult, BoxFuture, Capabilities},
};

/// A provider that answers from a script instead of a model.
///
/// It exists for three reasons: tests that must not touch the network,
/// developers working offline, and anyone who wants to see how the AI sidebar
/// behaves before deciding to give a vendor their data.
pub struct MockProvider {
    name: String,
    replies: Mutex<Vec<Completion>>,
    seen: Mutex<Vec<CompletionRequest>>,
}

impl MockProvider {
    /// A mock that echoes the last thing it was told.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            replies: Mutex::new(Vec::new()),
            seen: Mutex::new(Vec::new()),
        }
    }

    /// Queues replies, which are handed out in order. Once they run out the
    /// mock goes back to echoing.
    #[must_use]
    pub fn with_replies(self, replies: Vec<Completion>) -> Self {
        *self.replies.lock().expect("no other thread holds this") = replies;
        self
    }

    /// Every request the mock has been given, for tests that care what was
    /// actually asked.
    #[must_use]
    pub fn requests(&self) -> Vec<CompletionRequest> {
        self.seen
            .lock()
            .expect("no other thread holds this")
            .clone()
    }
}

impl AiProvider for MockProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            tools: true,
            embeddings: true,
            local: true,
        }
    }

    fn complete<'a>(&'a self, request: CompletionRequest) -> BoxFuture<'a, AiResult<Completion>> {
        let queued = {
            let mut replies = self.replies.lock().expect("no other thread holds this");
            if replies.is_empty() {
                None
            } else {
                Some(replies.remove(0))
            }
        };
        self.seen
            .lock()
            .expect("no other thread holds this")
            .push(request.clone());

        Box::pin(async move {
            Ok(queued.unwrap_or_else(|| {
                let last = request
                    .messages
                    .iter()
                    .rev()
                    .find(|message| message.role == Role::User)
                    .map_or_else(String::new, |message: &ChatMessage| message.content.clone());
                Completion {
                    content: format!("You said: {last}"),
                    tool_calls: Vec::new(),
                    finish_reason: FinishReason::Stop,
                    usage: None,
                }
            }))
        })
    }

    fn embed<'a>(&'a self, texts: Vec<String>) -> BoxFuture<'a, AiResult<Vec<Embedding>>> {
        // A deterministic, meaningless vector: enough to exercise the
        // plumbing, obviously not enough to be mistaken for real embeddings.
        Box::pin(async move {
            Ok(texts
                .iter()
                .map(|text| Embedding {
                    values: vec![text.len() as f32, text.chars().count() as f32],
                })
                .collect())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs a future to completion without pulling in an async runtime.
    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        use std::task::{Context, Poll, Wake, Waker};
        struct Noop;
        impl Wake for Noop {
            fn wake(self: std::sync::Arc<Self>) {}
        }
        let waker = Waker::from(std::sync::Arc::new(Noop));
        let mut context = Context::from_waker(&waker);
        let mut future = Box::pin(future);
        loop {
            if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
                return value;
            }
        }
    }

    #[test]
    fn the_mock_echoes_when_it_has_nothing_queued() {
        let provider = MockProvider::new("mock");
        let reply = block_on(provider.complete(CompletionRequest::new(
            "any",
            vec![ChatMessage::user("hello")],
        )))
        .unwrap();
        assert_eq!(reply.content, "You said: hello");
    }

    #[test]
    fn queued_replies_are_handed_out_in_order() {
        let provider = MockProvider::new("mock")
            .with_replies(vec![Completion::text("first"), Completion::text("second")]);
        let request = CompletionRequest::new("any", vec![ChatMessage::user("x")]);
        assert_eq!(
            block_on(provider.complete(request.clone()))
                .unwrap()
                .content,
            "first"
        );
        assert_eq!(
            block_on(provider.complete(request.clone()))
                .unwrap()
                .content,
            "second"
        );
        assert!(
            block_on(provider.complete(request))
                .unwrap()
                .content
                .starts_with("You said"),
            "it falls back to echoing once the script runs out"
        );
    }

    #[test]
    fn the_mock_remembers_what_it_was_asked() {
        let provider = MockProvider::new("mock");
        block_on(provider.complete(CompletionRequest::new(
            "any",
            vec![ChatMessage::user("remember me")],
        )))
        .unwrap();
        assert_eq!(provider.requests().len(), 1);
    }
}
