//! Talking to an MCP server somebody else wrote.
//!
//! The same protocol as [`Server`](crate::Server), pointed the other way, so
//! a stack can reach the rest of the world without HyperLab having to know
//! what is out there.
//!
//! # What this treats as hostile
//!
//! An external server is a program on the far end of a pipe. It is not part
//! of HyperLab, it may be a mistake, and it may be worse than a mistake, so:
//!
//! * **It is launched, never interpreted.** [`Launch`] holds a program and a
//!   list of arguments which are passed to the operating system as they
//!   stand. There is no shell, so there is nothing for a quotation mark in a
//!   stack to escape from.
//! * **It cannot hang HyperLab.** Every reply is waited for with a timeout,
//!   on a thread of its own, so a server that stops answering costs one
//!   call rather than the application.
//! * **It cannot exhaust memory.** A line longer than [`MAX_LINE`] ends the
//!   conversation instead of being buffered.
//! * **What it says is data.** Tool descriptions and results are text from
//!   somewhere else. They are carried, never obeyed, and never merged into
//!   HyperLab's own tool table — [`ExternalTool`] keeps the server's name
//!   attached so it is always clear whose tool is whose.

use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{Receiver, RecvTimeoutError, channel},
    thread::JoinHandle,
    time::Duration,
};

use serde_json::{Value as Json, json};

use crate::{
    error::{ToolError, ToolResult},
    jsonrpc::{Request, RequestId, RpcError},
    server::PROTOCOL_VERSION,
};

/// The longest line an external server may send.
pub const MAX_LINE: usize = 16 * 1024 * 1024;

/// How long to wait for a reply before giving up on it.
pub const PATIENCE: Duration = Duration::from_secs(30);

/// How long a server is given to finish after its input closes.
///
/// A server may have work to do once the conversation ends — HyperLab's own
/// saves the stack — so it is told the session is over and then left alone
/// for a moment. Killing it immediately would lose whatever it was writing.
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// How to start an external MCP server.
///
/// A program and its arguments, handed to the operating system unchanged.
/// There is deliberately no field for a command line to be parsed: the moment
/// one string becomes a program *and* its arguments, whatever built that
/// string has to be trusted, and nothing that reaches here is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    /// The program to run.
    pub program: String,
    /// Its arguments, one per element, passed through untouched.
    pub arguments: Vec<String>,
    /// Environment variables to set for it.
    ///
    /// The child otherwise inherits this process's environment, which is
    /// where API keys live — so a server that has no business seeing them
    /// should be given [`Launch::with_clean_environment`].
    pub environment: BTreeMap<String, String>,
    /// Whether to hand the child a fresh environment rather than this one.
    pub clean_environment: bool,
}

impl Launch {
    /// Runs `program` with no arguments.
    #[must_use]
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            arguments: Vec::new(),
            environment: BTreeMap::new(),
            clean_environment: false,
        }
    }

    /// Adds arguments, each passed to the program as one argument.
    #[must_use]
    pub fn arguments<I, S>(mut self, arguments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.arguments.extend(arguments.into_iter().map(Into::into));
        self
    }

    /// Sets one environment variable for the child.
    #[must_use]
    pub fn variable(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.environment.insert(name.into(), value.into());
        self
    }

    /// Starts the child with an empty environment rather than this one.
    ///
    /// Worth doing for anything that has no reason to read this process's
    /// variables, since those are where every provider's key is kept.
    #[must_use]
    pub const fn with_clean_environment(mut self) -> Self {
        self.clean_environment = true;
        self
    }
}

/// A tool an external server offers.
///
/// The server's name stays attached, because two servers may well both offer
/// a `search` and a person deciding whether to allow one needs to know whose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalTool {
    /// Which server offers it.
    pub server: String,
    /// What the server calls it.
    pub name: String,
    /// What the server says it does. Text from elsewhere; describe it to a
    /// person, do not act on it.
    pub description: String,
    /// The JSON Schema the server published for its arguments.
    pub input_schema: Json,
}

impl ExternalTool {
    /// A name that cannot collide with HyperLab's own tools, or another
    /// server's.
    #[must_use]
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.server, self.name)
    }
}

/// A running external MCP server.
///
/// Dropping this shuts the server down.
pub struct Client {
    name: String,
    child: Child,
    /// Taken to close the pipe, which is how a server is told to finish.
    stdin: Option<ChildStdin>,
    replies: Receiver<std::io::Result<String>>,
    reader: Option<JoinHandle<()>>,
    next_id: i64,
}

impl Client {
    /// Starts a server and completes the MCP handshake.
    ///
    /// # Errors
    ///
    /// Returns [`ToolError::Runtime`] if the program cannot be started, does
    /// not answer within [`PATIENCE`], or answers with something that is not
    /// an MCP initialization.
    pub fn start(name: impl Into<String>, launch: &Launch) -> ToolResult<Self> {
        let name = name.into();

        let mut command = Command::new(&launch.program);
        command
            .args(&launch.arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // The child's own diagnostics are its business, and must not be
            // mistaken for protocol if it writes to the wrong stream.
            .stderr(Stdio::null());
        if launch.clean_environment {
            command.env_clear();
        }
        command.envs(&launch.environment);

        let mut child = command.spawn().map_err(|error| {
            ToolError::Runtime(format!("could not start \"{}\": {error}", launch.program))
        })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            ToolError::Runtime(format!("\"{}\" has no standard input", launch.program))
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            ToolError::Runtime(format!("\"{}\" has no standard output", launch.program))
        })?;

        // Reading happens on its own thread so that a server which stops
        // talking costs a timeout rather than the application.
        let (sender, replies) = channel();
        let reader = std::thread::spawn(move || {
            let mut lines = BufReader::new(stdout).take(MAX_LINE as u64);
            loop {
                let mut line = String::new();
                let outcome = lines.read_line(&mut line);
                lines.set_limit(MAX_LINE as u64);
                match outcome {
                    Ok(0) => return,
                    Ok(_) => {
                        if sender.send(Ok(line)).is_err() {
                            return;
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        return;
                    }
                }
            }
        });

        let mut client = Self {
            name,
            child,
            stdin: Some(stdin),
            replies,
            reader: Some(reader),
            next_id: 1,
        };
        client.handshake()?;
        Ok(client)
    }

    /// Ends the session and waits for the server to finish.
    ///
    /// Closing the pipe is how a server is told there is nothing more coming;
    /// it then has [`SHUTDOWN_GRACE`] to do whatever it does at the end — for
    /// HyperLab's own server, saving the stack — before it is killed.
    ///
    /// Dropping a client does this too. Call it directly when you need to
    /// know the server has finished before carrying on.
    pub fn shutdown(&mut self) {
        // Dropping the handle closes the pipe. Nothing else does.
        drop(self.stdin.take());

        let deadline = std::time::Instant::now() + SHUTDOWN_GRACE;
        loop {
            match self.child.try_wait() {
                // Gone of its own accord, having done whatever it needed to.
                Ok(Some(_)) => break,
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                // Out of patience, or we cannot tell: a server that ignores
                // end-of-input must not outlive the application.
                Ok(None) | Err(_) => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
            }
        }

        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }

    /// What this client calls the server it is talking to.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Asks the server what it offers.
    ///
    /// # Errors
    ///
    /// Returns [`ToolError::Runtime`] if the server does not answer, or
    /// answers with something that is not a tool list.
    pub fn tools(&mut self) -> ToolResult<Vec<ExternalTool>> {
        let reply = self.call("tools/list", json!({}))?;
        let listed = reply
            .get("tools")
            .and_then(Json::as_array)
            .ok_or_else(|| self.confused("its tool list had no tools in it"))?;

        Ok(listed
            .iter()
            .filter_map(|tool| {
                Some(ExternalTool {
                    server: self.name.clone(),
                    name: tool.get("name")?.as_str()?.to_string(),
                    description: tool
                        .get("description")
                        .and_then(Json::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    input_schema: tool
                        .get("inputSchema")
                        .cloned()
                        .unwrap_or_else(|| json!({ "type": "object" })),
                })
            })
            .collect())
    }

    /// Calls one of the server's tools.
    ///
    /// The reply is returned as text, which is what MCP tool results are.
    /// A tool that reports its own failure comes back as `Ok`: that is the
    /// server saying no, not the connection breaking.
    ///
    /// # Errors
    ///
    /// Returns [`ToolError::Runtime`] if the server does not answer within
    /// [`PATIENCE`], or answers with a protocol error.
    pub fn call_tool(&mut self, tool: &str, arguments: &Json) -> ToolResult<String> {
        let reply = self.call(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
        )?;
        Ok(text_of(&reply))
    }

    fn handshake(&mut self) -> ToolResult<()> {
        self.call(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "hyperlab", "version": env!("CARGO_PKG_VERSION") },
            }),
        )?;
        self.notify("notifications/initialized")
    }

    fn notify(&mut self, method: &str) -> ToolResult<()> {
        self.send(&Request::notify(method, None))
    }

    fn call(&mut self, method: &str, params: Json) -> ToolResult<Json> {
        let id = self.next_id;
        self.next_id += 1;

        self.send(&Request::call(RequestId::Number(id), method, Some(params)))?;

        // A server may interleave its own requests and notifications with
        // its answers, so read until the id we are waiting for turns up.
        loop {
            let line = self.receive()?;
            let Ok(message) = serde_json::from_str::<Json>(&line) else {
                return Err(self.confused("it said something that was not JSON"));
            };

            match message.get("id").and_then(Json::as_i64) {
                Some(answered) if answered == id => {
                    if let Some(error) = message.get("error") {
                        let detail = error
                            .get("message")
                            .and_then(Json::as_str)
                            .unwrap_or("no reason given");
                        return Err(self.confused(&format!("it refused: {detail}")));
                    }
                    return Ok(message.get("result").cloned().unwrap_or(Json::Null));
                }
                // Somebody else's answer, or a notification. Not ours.
                _ => continue,
            }
        }
    }

    fn send(&mut self, request: &Request) -> ToolResult<()> {
        let line = serde_json::to_string(request)
            .map_err(|error| ToolError::Runtime(format!("could not encode a request: {error}")))?;
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(ToolError::Runtime(format!(
                "the MCP server \"{}\" has already been shut down",
                self.name
            )));
        };
        writeln!(stdin, "{line}")
            .and_then(|()| stdin.flush())
            .map_err(|error| self.gone(&format!("could not be written to: {error}")))
    }

    fn receive(&mut self) -> ToolResult<String> {
        match self.replies.recv_timeout(PATIENCE) {
            Ok(Ok(line)) => Ok(line),
            Ok(Err(error)) => Err(self.gone(&format!("could not be read: {error}"))),
            Err(RecvTimeoutError::Timeout) => Err(self.gone(&format!(
                "did not answer within {} seconds",
                PATIENCE.as_secs()
            ))),
            Err(RecvTimeoutError::Disconnected) => Err(self.gone("stopped")),
        }
    }

    fn confused(&self, detail: &str) -> ToolError {
        ToolError::Runtime(format!("the MCP server \"{}\" {detail}", self.name))
    }

    fn gone(&self, detail: &str) -> ToolError {
        ToolError::Runtime(format!("the MCP server \"{}\" {detail}", self.name))
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// Pulls the text out of an MCP tool result.
fn text_of(reply: &Json) -> String {
    let Some(content) = reply.get("content").and_then(Json::as_array) else {
        return compact(reply);
    };

    let text: Vec<&str> = content
        .iter()
        .filter(|block| block.get("type").and_then(Json::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Json::as_str))
        .collect();

    if text.is_empty() {
        compact(reply)
    } else {
        text.join("\n")
    }
}

fn compact(value: &Json) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

/// Turns a JSON-RPC error into something worth showing.
impl From<RpcError> for ToolError {
    fn from(error: RpcError) -> Self {
        Self::Runtime(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_launch_keeps_arguments_separate_so_there_is_nothing_to_escape() {
        let launch = Launch::new("mcp-server").arguments(["--root", "/tmp/a b; rm -rf /"]);

        // The dangerous-looking argument is one argument. It reaches the
        // program as a single string and no shell ever sees it.
        assert_eq!(launch.program, "mcp-server");
        assert_eq!(launch.arguments, vec!["--root", "/tmp/a b; rm -rf /"]);
    }

    #[test]
    fn a_clean_environment_is_opt_in_and_recorded() {
        let ordinary = Launch::new("x");
        assert!(!ordinary.clean_environment);

        let careful = Launch::new("x")
            .with_clean_environment()
            .variable("TOKEN", "abc");
        assert!(careful.clean_environment);
        assert_eq!(
            careful.environment.get("TOKEN").map(String::as_str),
            Some("abc")
        );
    }

    #[test]
    fn a_tool_keeps_the_name_of_the_server_that_offered_it() {
        let tool = ExternalTool {
            server: "files".into(),
            name: "search".into(),
            description: "Search".into(),
            input_schema: json!({}),
        };
        assert_eq!(tool.qualified_name(), "files.search");
    }

    #[test]
    fn a_tool_result_is_read_out_of_its_content_blocks() {
        let reply = json!({
            "content": [
                { "type": "text", "text": "first" },
                { "type": "image", "data": "ignored" },
                { "type": "text", "text": "second" },
            ]
        });
        assert_eq!(text_of(&reply), "first\nsecond");
    }

    #[test]
    fn a_result_with_no_text_is_shown_as_it_arrived_rather_than_as_nothing() {
        let reply = json!({ "content": [{ "type": "image", "data": "..." }] });
        assert!(text_of(&reply).contains("image"));
    }

    #[test]
    fn a_program_that_does_not_exist_is_reported_rather_than_panicking() {
        let error = Client::start("ghost", &Launch::new("hyperlab-no-such-program-exists"))
            .expect_err("starting a missing program must fail");
        assert!(matches!(error, ToolError::Runtime(_)), "got {error:?}");
    }
}
