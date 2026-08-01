//! Serving HyperLab's tools to an MCP client.
//!
//! The protocol is small: a client says hello, asks what there is, and calls
//! things. This module is that conversation and nothing else — it reads from
//! a [`BufRead`] and writes to a [`Write`], so the tests drive it over a pair
//! of in-memory buffers and the binary hands it stdin and stdout. Nothing
//! here knows what a pipe is.
//!
//! Every call goes through a [`Policy`] first. That is the whole reason the
//! server does not simply forward to [`ToolRegistry`]: a caller on the far
//! end of a pipe is not the user, and must not be treated as though it were.

use std::io::{BufRead, Write};

use hyperlab_runtime::Runtime;
use hyperlab_stack::Object;
use serde_json::{Value as Json, json};

use crate::{
    error::ToolError,
    jsonrpc::{Request, RequestId, Response, RpcError, parse_request},
    permission::{Approver, Decision, DenyAll, Policy, Verdict},
    registry::ToolRegistry,
};

/// The revision of MCP this speaks.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// A line longer than this is refused rather than buffered.
///
/// A peer that never sends a newline would otherwise grow this process until
/// the machine gives out. Sixteen megabytes is far more than any honest tool
/// call and far less than a problem.
const MAX_LINE: u64 = 16 * 1024 * 1024;

/// What the server calls itself when a client asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInfo {
    /// The implementation name.
    pub name: String,
    /// Its version.
    pub version: String,
}

impl Default for ServerInfo {
    fn default() -> Self {
        Self {
            name: "hyperlab".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// Serves HyperLab's tools over MCP.
pub struct Server {
    registry: ToolRegistry,
    policy: Policy,
    info: ServerInfo,
    decisions: Vec<Decision>,
    initialized: bool,
}

impl Default for Server {
    fn default() -> Self {
        Self::new(Policy::new())
    }
}

impl Server {
    /// A server that enforces `policy`.
    #[must_use]
    pub fn new(policy: Policy) -> Self {
        Self {
            registry: ToolRegistry::new(),
            policy,
            info: ServerInfo::default(),
            decisions: Vec::new(),
            initialized: false,
        }
    }

    /// Changes what the server calls itself.
    #[must_use]
    pub fn described_as(mut self, info: ServerInfo) -> Self {
        self.info = info;
        self
    }

    /// Every permission decision made so far, oldest first.
    ///
    /// This is the record of what was allowed, what was refused and what a
    /// person was asked — the thing you show someone who wants to know what
    /// a connection actually did.
    #[must_use]
    pub fn decisions(&self) -> &[Decision] {
        &self.decisions
    }

    /// The policy, so a caller can widen or narrow it while running.
    pub fn policy_mut(&mut self) -> &mut Policy {
        &mut self.policy
    }

    /// Reads requests until the input ends, answering each one.
    ///
    /// Returns when the client hangs up, which is the ordinary way an MCP
    /// session over a pipe finishes.
    ///
    /// # Errors
    ///
    /// Returns the first I/O error from reading or writing. A malformed
    /// *request* is not an error: it is answered with one, and the loop
    /// carries on, because the next line is usually fine.
    pub fn serve(
        &mut self,
        runtime: &mut Runtime,
        input: impl BufRead,
        mut output: impl Write,
        approver: &mut dyn Approver,
    ) -> std::io::Result<()> {
        let mut input = input.take(MAX_LINE);
        let mut line = String::new();

        loop {
            line.clear();
            if input.read_line(&mut line)? == 0 {
                return Ok(());
            }
            // `take` is a budget for the whole reader, not per line, so it
            // has to be handed back after each one.
            input.set_limit(MAX_LINE);

            if line.trim().is_empty() {
                continue;
            }

            if let Some(response) = self.handle_line(runtime, &line, approver) {
                serde_json::to_writer(&mut output, &response)?;
                output.write_all(b"\n")?;
                output.flush()?;
            }
        }
    }

    /// Answers one line of input, or nothing if it was a notification.
    fn handle_line(
        &mut self,
        runtime: &mut Runtime,
        line: &str,
        approver: &mut dyn Approver,
    ) -> Option<Response> {
        match parse_request(line) {
            Ok(request) => self.handle(runtime, &request, approver),
            // A request we could not parse has no id to answer, so the only
            // honest reply is one with a null id, which the specification
            // allows precisely for this case.
            Err(error) => Some(Response::failure(RequestId::Number(0), error)),
        }
    }

    /// Answers one request, or nothing if it was a notification.
    pub fn handle(
        &mut self,
        runtime: &mut Runtime,
        request: &Request,
        approver: &mut dyn Approver,
    ) -> Option<Response> {
        let outcome = self.dispatch(runtime, request, approver);

        // A notification must never be answered, not even to complain: the
        // client is not listening for one, and an unexpected response would
        // be matched against somebody else's call.
        let id = request.id.clone()?;
        Some(match outcome {
            Ok(result) => Response::success(id, result),
            Err(error) => Response::failure(id, error),
        })
    }

    fn dispatch(
        &mut self,
        runtime: &mut Runtime,
        request: &Request,
        approver: &mut dyn Approver,
    ) -> Result<Json, RpcError> {
        match request.method.as_str() {
            "initialize" => {
                self.initialized = true;
                Ok(json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": self.info.name, "version": self.info.version },
                }))
            }
            "notifications/initialized" => Ok(Json::Null),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": self.describe_tools() })),
            "tools/call" => self.call_tool(runtime, request.params.as_ref(), approver),
            other => Err(RpcError::method_not_found(other)),
        }
    }

    /// The tools this connection may actually use.
    ///
    /// A policy that forbids a tool hides it as well as refusing it. Offering
    /// a model a tool it is not allowed to call wastes a turn and reads, to
    /// anything watching, like the model misbehaving.
    fn describe_tools(&self) -> Vec<Json> {
        crate::tools::TOOLS
            .iter()
            .filter(|tool| !self.policy.would_always_refuse(tool.name, tool.access))
            .map(|tool| {
                let definition = tool.definition();
                json!({
                    "name": definition.name,
                    "description": definition.description,
                    "inputSchema": definition.input_schema,
                })
            })
            .collect()
    }

    fn call_tool(
        &mut self,
        runtime: &mut Runtime,
        params: Option<&Json>,
        approver: &mut dyn Approver,
    ) -> Result<Json, RpcError> {
        let params = params.ok_or_else(|| RpcError::invalid_params("no arguments were given"))?;
        let name = params
            .get("name")
            .and_then(Json::as_str)
            .ok_or_else(|| RpcError::invalid_params("a tool call needs a name"))?;

        // Missing arguments are an empty object, not an error: a tool that
        // takes none is called without them by most clients.
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));

        let tool = self.registry.get(name).ok_or_else(|| {
            RpcError::invalid_params(format!("there is no tool called \"{name}\""))
        })?;

        let stack = runtime.stack().name().to_string();
        let decision = self.policy.decide(name, tool.access, &stack, approver);
        let refusal = match &decision.verdict {
            Verdict::Allowed => None,
            Verdict::Refused { reason } => Some(reason.clone()),
        };
        self.decisions.push(decision);

        // A refusal is a tool result, not a protocol error: the model has to
        // see it and choose something else, and a JSON-RPC error would
        // usually be swallowed before it got there.
        if let Some(reason) = refusal {
            return Ok(failed(&reason));
        }

        match self.registry.call(runtime, name, &arguments) {
            Ok(value) => Ok(json!({
                "content": [{ "type": "text", "text": compact(&value) }],
                "structuredContent": value,
                "isError": false,
            })),
            // A tool that refused its arguments, or a script that failed, is
            // news for the model rather than a broken connection.
            Err(error @ (ToolError::BadArguments(_) | ToolError::Runtime(_))) => {
                Ok(failed(&error.to_string()))
            }
            Err(error @ ToolError::UnknownTool(_)) => Err(RpcError::internal(error.to_string())),
        }
    }
}

/// A tool result that says something went wrong.
fn failed(reason: &str) -> Json {
    json!({
        "content": [{ "type": "text", "text": reason }],
        "isError": true,
    })
}

/// Renders a tool's answer as the text an MCP client expects alongside it.
fn compact(value: &Json) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

/// Serves one session over the process's own stdin and stdout.
///
/// The whole of the stdio transport: MCP puts one JSON object per line on a
/// pipe, so there is nothing to it beyond handing the streams over.
///
/// Nothing may be printed to stdout by anything else while this runs — it is
/// the protocol channel, and a stray `println!` corrupts the session. Log to
/// stderr.
///
/// # Errors
///
/// Returns the first I/O error from either stream.
pub fn serve_stdio(
    server: &mut Server,
    runtime: &mut Runtime,
    approver: &mut dyn Approver,
) -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    server.serve(runtime, stdin.lock(), stdout.lock(), approver)
}

/// Serves a session that refuses anything needing consent.
///
/// # Errors
///
/// Returns the first I/O error from either stream.
pub fn serve_stdio_unattended(server: &mut Server, runtime: &mut Runtime) -> std::io::Result<()> {
    serve_stdio(server, runtime, &mut DenyAll)
}

#[cfg(test)]
mod tests {
    use hyperlab_stack::Stack;

    use super::*;
    use crate::permission::AllowAll;

    /// Runs a session and returns one parsed response per line written.
    fn session(policy: Policy, input: &str) -> Vec<Json> {
        let mut runtime = Runtime::new(Stack::new("Test"));
        let mut server = Server::new(policy);
        let mut output = Vec::new();
        server
            .serve(&mut runtime, input.as_bytes(), &mut output, &mut AllowAll)
            .expect("a session over buffers cannot fail");
        String::from_utf8(output)
            .expect("the server writes UTF-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("each line is a JSON response"))
            .collect()
    }

    fn call(name: &str, arguments: Json) -> String {
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments },
        })
        .to_string()
    }

    #[test]
    fn a_client_is_told_the_protocol_version_and_who_it_is_talking_to() {
        let replies = session(
            Policy::new(),
            &format!(
                "{}\n",
                json!({"jsonrpc":"2.0","id":1,"method":"initialize"})
            ),
        );

        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0]["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(replies[0]["result"]["serverInfo"]["name"], "hyperlab");
        assert!(replies[0]["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn a_notification_is_never_answered() {
        let replies = session(
            Policy::new(),
            &format!(
                "{}\n{}\n",
                json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
                json!({"jsonrpc":"2.0","id":9,"method":"ping"}),
            ),
        );

        // Only the ping is answered, and it keeps its own id: a response to
        // the notification would be matched against the wrong call.
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0]["id"], 9);
    }

    #[test]
    fn tools_are_listed_in_the_shape_the_protocol_asks_for() {
        let replies = session(
            Policy::trusted(),
            &format!(
                "{}\n",
                json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})
            ),
        );

        let tools = replies[0]["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), crate::tools::TOOLS.len());
        assert!(tools.iter().all(|tool| {
            tool["name"].is_string()
                && tool["description"].is_string()
                && tool["inputSchema"]["type"] == "object"
        }));
    }

    #[test]
    fn a_read_only_connection_is_not_shown_the_tools_it_could_not_call() {
        let replies = session(
            Policy::new(),
            &format!(
                "{}\n",
                json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})
            ),
        );

        let names: Vec<&str> = replies[0]["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();

        assert!(names.contains(&"read_field"));
        assert!(
            !names.contains(&"write_field"),
            "offering a tool it may not call wastes the model's turn: {names:?}"
        );
    }

    #[test]
    fn a_tool_call_returns_both_text_and_the_structure_behind_it() {
        let replies = session(
            Policy::trusted(),
            &format!("{}\n", call("create_field", json!({ "name": "Title" }))),
        );

        let result = &replies[0]["result"];
        assert_eq!(result["isError"], false);
        assert_eq!(result["content"][0]["type"], "text");
        assert!(result["structuredContent"]["id"].is_number());
    }

    #[test]
    fn a_refused_tool_is_an_error_the_model_can_read_not_a_broken_connection() {
        let replies = session(
            Policy::new(),
            &format!("{}\n", call("create_card", json!({}))),
        );

        // A JSON-RPC error would usually be swallowed by the client before
        // the model ever saw it.
        assert!(replies[0]["error"].is_null(), "got {}", replies[0]);
        assert_eq!(replies[0]["result"]["isError"], true);
        assert!(
            replies[0]["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("may only read")
        );
    }

    #[test]
    fn a_tool_that_dislikes_its_arguments_says_so_without_failing_the_call() {
        let replies = session(
            Policy::trusted(),
            &format!("{}\n", call("write_field", json!({ "name": "Nope" }))),
        );

        assert!(replies[0]["error"].is_null());
        assert_eq!(replies[0]["result"]["isError"], true);
    }

    #[test]
    fn asking_for_a_tool_that_does_not_exist_is_a_protocol_error() {
        let replies = session(
            Policy::trusted(),
            &format!("{}\n", call("teleport", json!({}))),
        );
        assert_eq!(replies[0]["error"]["code"], RpcError::INVALID_PARAMS);
    }

    #[test]
    fn an_unknown_method_is_reported_as_one() {
        let replies = session(
            Policy::trusted(),
            &format!(
                "{}\n",
                json!({"jsonrpc":"2.0","id":1,"method":"resources/list"})
            ),
        );
        assert_eq!(replies[0]["error"]["code"], RpcError::METHOD_NOT_FOUND);
    }

    #[test]
    fn a_torn_line_is_answered_and_the_session_carries_on() {
        let replies = session(
            Policy::trusted(),
            &format!(
                "{{not json\n\n{}\n",
                json!({"jsonrpc":"2.0","id":2,"method":"ping"})
            ),
        );

        assert_eq!(
            replies.len(),
            2,
            "the blank line should be skipped silently"
        );
        assert_eq!(replies[0]["error"]["code"], RpcError::PARSE_ERROR);
        assert_eq!(replies[1]["id"], 2);
    }

    #[test]
    fn what_a_connection_was_allowed_to_do_is_kept() {
        let mut runtime = Runtime::new(Stack::new("Test"));
        let mut server = Server::new(Policy::new());
        let mut output = Vec::new();
        let input = format!(
            "{}\n{}\n",
            call("list_cards", json!({})),
            call("create_card", json!({}))
        );

        server
            .serve(&mut runtime, input.as_bytes(), &mut output, &mut AllowAll)
            .unwrap();

        let decisions = server.decisions();
        assert_eq!(decisions.len(), 2);
        assert_eq!(decisions[0].tool, "list_cards");
        assert!(decisions[0].verdict.is_allowed());
        assert_eq!(decisions[1].tool, "create_card");
        assert!(!decisions[1].verdict.is_allowed());
    }

    #[test]
    fn a_tool_call_really_does_change_the_stack_and_can_be_taken_back() {
        let mut runtime = Runtime::new(Stack::new("Test"));
        let mut server = Server::new(Policy::trusted());
        let before = runtime.stack().card_count();

        server
            .serve(
                &mut runtime,
                call("create_card", json!({})).as_bytes(),
                &mut Vec::new(),
                &mut AllowAll,
            )
            .unwrap();

        assert_eq!(runtime.stack().card_count(), before + 1);
        assert!(runtime.undo().unwrap());
        assert_eq!(runtime.stack().card_count(), before);
    }
}
