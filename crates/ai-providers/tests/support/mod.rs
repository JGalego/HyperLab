//! A one-file HTTP server, so the providers can be tested over a socket.
//!
//! The unit tests in each provider check the JSON going out and the JSON
//! coming in. What they cannot check is that the two halves meet: that the
//! request lands on the right path with the right headers, and that the reply
//! is read back off the wire. This does that, without a network, a mocking
//! framework or a dependency.

use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

use serde_json::Value;

/// A request the server received.
#[derive(Debug, Clone)]
pub struct Recorded {
    /// The path asked for, such as `/v1/messages`.
    pub path: String,
    /// The headers, with their names lowercased.
    pub headers: BTreeMap<String, String>,
    /// The body, parsed as JSON.
    pub body: Value,
}

impl Recorded {
    /// One header, or `None` if it was not sent.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }
}

/// A server that answers a fixed number of requests and remembers them.
pub struct MockServer {
    base_url: String,
    received: Arc<Mutex<Vec<Recorded>>>,
    serving: Option<JoinHandle<()>>,
}

impl MockServer {
    /// Starts a server that answers `200 OK` with `body`, once.
    pub fn replying(body: &str) -> Self {
        Self::answering(vec![(200, body.to_string())])
    }

    /// Starts a server that gives each reply in turn, then stops.
    ///
    /// # Panics
    ///
    /// Panics if no port can be bound, which means the test cannot run.
    pub fn answering(replies: Vec<(u16, String)>) -> Self {
        // Port 0 asks the operating system for a free one, so tests running
        // side by side cannot collide.
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let base_url = format!("http://{}", listener.local_addr().expect("an address"));

        let received = Arc::new(Mutex::new(Vec::new()));
        let recording = Arc::clone(&received);
        let serving = thread::spawn(move || {
            for (status, body) in replies {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                if let Some(request) = read_request(&mut stream) {
                    recording.lock().expect("no panic held this").push(request);
                }
                let _ = write_reply(&mut stream, status, &body);
            }
        });

        Self {
            base_url,
            received,
            serving: Some(serving),
        }
    }

    /// Where to point a provider.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Everything received so far, in order.
    pub fn received(&self) -> Vec<Recorded> {
        self.received.lock().expect("no panic held this").clone()
    }

    /// The only request received.
    ///
    /// # Panics
    ///
    /// Panics unless exactly one request arrived, which is a clearer failure
    /// than an assertion against the wrong one.
    pub fn only_request(&self) -> Recorded {
        let received = self.received();
        assert_eq!(received.len(), 1, "expected exactly one request");
        received.into_iter().next().expect("just counted it")
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        // The thread stops on its own once it has given out every reply. A
        // test that asked for more replies than it used would hang here, so
        // it is left detached rather than joined.
        drop(self.serving.take());
    }
}

/// Reads one HTTP request. Returns `None` if the connection said nothing
/// usable, which is not worth failing a test over.
fn read_request(stream: &mut TcpStream) -> Option<Recorded> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);

    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;
    let path = request_line.split_whitespace().nth(1)?.to_string();

    let mut headers = BTreeMap::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_lowercase(), value.trim().to_string());
        }
    }

    let length: usize = headers
        .get("content-length")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;

    Some(Recorded {
        path,
        headers,
        body: serde_json::from_slice(&body).unwrap_or(Value::Null),
    })
}

/// Writes one HTTP reply and closes the connection, so the next request
/// arrives as a fresh one this server can count.
fn write_reply(stream: &mut TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status} \r\n\
         content-type: application/json\r\n\
         content-length: {}\r\n\
         connection: close\r\n\
         \r\n\
         {body}",
        body.len()
    )?;
    stream.flush()
}
