//! A tiny HTTP server on 127.0.0.1 so a web page on this Mac (a web app with a dictation button,
//! e.g. agent-fleet's) can ask Dictate to
//! start or stop a recording and see whether Dictate is there at all.
//!
//! - `GET /status` → `{"app":"dictate","version":…,"ready":bool,"recording":bool}`
//! - `POST /toggle` → starts a hands-free recording or stops the running one (like a tap on a
//!   toggle trigger); the text is pasted at the cursor as always. `{"ok":bool,"ready":bool}`
//! - `POST /talk?session=<id>` → a spoken conversation with that session of the profile's agent
//!   plugin (`talk.rs`, `docs/agent-plugins.md`; like
//!   "Darüber diskutieren", but the session exists and stays open). `{"ok":bool,"ready":bool}`
//! - `POST /talk/stop` → ends the conversation (or a reading). `{"ok":true,"ready":bool}`
//! - `POST /souffleur` → starts the Souffleur (or shows its panel when it runs);
//!   `POST /souffleur/stop` ends it. `{"ok":true,"ready":bool}` (Raycast, `dictate souffleur start`).
//! - `/status` also carries `"talk"`: `null`, or `{"session":<id>|null,"phase":"listening"|
//!   "thinking"|"speaking"|"reading"}` while a conversation or reading runs.
//!
//! Only pages the profile lists (`web_origins`, see [`origins`]) and loopback pages (development)
//! may call it: a browser sends
//! `Origin` with every cross-site request, and any other one is refused with 403, before anything
//! happens. `Host` must name the loopback address (DNS rebinding). Requests without `Origin`
//! (curl, scripts of the owner) pass. Chrome's private-network preflight is answered too.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::platform::{RECORDING, TapEvent};

/// The port; fixed so the page knows where to ask.
pub const PORT: u16 = 47619;

/// Pages that may call the server (besides loopback pages): the profile's `web_origins`, e.g. a
/// web app's https origin and `tauri://localhost` for a Tauri app on this Mac (its HTTP plugin
/// sends the webview's origin, and only an app on this Mac can send that one, which could call
/// the server without any `Origin` anyway). The tests use such a list.
fn origins() -> Vec<String> {
    if cfg!(test) {
        return ["https://app.example.com", "tauri://localhost"]
            .map(String::from)
            .to_vec();
    }
    crate::profile::current().web_origins.clone()
}

/// The worker has the model loaded and the microphone checked.
pub static READY: AtomicBool = AtomicBool::new(false);

/// The spoken conversation as `/status` shows it (`talk.rs` keeps it current): its session (none
/// yet while a new one starts) and phase; `None` when nothing is spoken or heard.
pub static TALK: Mutex<Option<(Option<u64>, &'static str)>> = Mutex::new(None);

/// What an allowed `POST` asks the worker for.
#[derive(Debug, PartialEq)]
enum Act {
    Toggle,
    Talk(u64),
    EndTalk,
    /// Start the Souffleur (true; shows the panel when it runs) or end it (false).
    Souffleur(bool),
}

type OnSouffleur = Arc<dyn Fn(bool) + Send + Sync>;

/// Starts the server on its own thread; it only logs when the port is taken.
/// `souffleur` runs for `/souffleur` (true) and `/souffleur/stop` (false), on the server's thread.
pub fn serve(tx: Sender<TapEvent>, souffleur: impl Fn(bool) + Send + Sync + 'static) {
    let souffleur: OnSouffleur = Arc::new(souffleur);
    let listener = match TcpListener::bind(("127.0.0.1", PORT)) {
        Ok(l) => l,
        Err(e) => return eprintln!("local server: port {PORT}: {e}"),
    };
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let tx = tx.clone();
            let souffleur = souffleur.clone();
            std::thread::spawn(move || {
                let _ = handle(stream, &tx, &souffleur);
            });
        }
    });
}

fn handle(
    mut stream: TcpStream,
    tx: &Sender<TapEvent>,
    souffleur: &OnSouffleur,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    // Only the head matters (no endpoint reads a body); 8 KB is plenty.
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < 8192 {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let head = String::from_utf8_lossy(&buf);
    let reply = respond(&head, |act| {
        // The Souffleur loads its own models: it does not wait for the dictation's.
        if let Act::Souffleur(start) = act {
            souffleur(start);
            return true;
        }
        let ready = READY.load(Ordering::Relaxed);
        if ready {
            let _ = tx.send(match act {
                Act::Toggle => TapEvent::Toggle,
                Act::Talk(id) => TapEvent::DiscussSession(id),
                Act::EndTalk => TapEvent::EndTalk,
                Act::Souffleur(_) => unreachable!(),
            });
        }
        ready
    });
    stream.write_all(reply.as_bytes())
}

/// The whole HTTP answer for a request head; `act` runs for an allowed `POST` and says whether
/// Dictate was ready to take it.
fn respond(head: &str, act: impl FnOnce(Act) -> bool) -> String {
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or("").split(' ');
    let (method, path) = (first.next().unwrap_or(""), first.next().unwrap_or(""));
    let header = |name: &str| {
        head.split("\r\n").skip(1).find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim().eq_ignore_ascii_case(name).then(|| v.trim())
        })
    };
    let origin = header("origin");
    if let Some(origin) = origin
        && !origin_allowed(origin)
    {
        return answer(
            403,
            "Forbidden",
            None,
            false,
            r#"{"error":"origin not allowed"}"#,
        );
    }
    if !header("host").is_some_and(host_allowed) {
        return answer(
            403,
            "Forbidden",
            None,
            false,
            r#"{"error":"host not allowed"}"#,
        );
    }
    let private = header("access-control-request-private-network").is_some();
    let ready = READY.load(Ordering::Relaxed);
    let recording = RECORDING.load(Ordering::Relaxed);
    let (route, query) = path.split_once('?').unwrap_or((path, ""));
    let done = |ok: bool| {
        answer(
            200,
            "OK",
            origin,
            private,
            &format!(r#"{{"ok":{ok},"ready":{ready}}}"#),
        )
    };
    match (method, route) {
        ("OPTIONS", _) => answer(204, "No Content", origin, private, ""),
        ("GET", "/status") => {
            let talk = match *TALK.lock().unwrap_or_else(|e| e.into_inner()) {
                None => "null".to_string(),
                Some((session, phase)) => format!(
                    r#"{{"session":{},"phase":"{phase}"}}"#,
                    session.map_or("null".into(), |s| s.to_string())
                ),
            };
            let body = format!(
                r#"{{"app":"dictate","version":"{}-{}","ready":{ready},"recording":{recording},"talk":{talk}}}"#,
                env!("CARGO_PKG_VERSION"),
                env!("DICTATE_COMMIT")
            );
            answer(200, "OK", origin, private, &body)
        }
        ("POST", "/toggle") => done(act(Act::Toggle)),
        ("POST", "/talk") => {
            let session = query
                .split('&')
                .find_map(|kv| kv.strip_prefix("session="))
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|&id| id > 0);
            match session {
                Some(id) => done(act(Act::Talk(id))),
                None => answer(
                    400,
                    "Bad Request",
                    origin,
                    private,
                    r#"{"error":"session=<id> missing"}"#,
                ),
            }
        }
        ("POST", "/talk/stop") => {
            act(Act::EndTalk);
            done(true)
        }
        ("POST", "/souffleur") => {
            act(Act::Souffleur(true));
            done(true)
        }
        ("POST", "/souffleur/stop") => {
            act(Act::Souffleur(false));
            done(true)
        }
        _ => answer(
            404,
            "Not Found",
            origin,
            private,
            r#"{"error":"not found"}"#,
        ),
    }
}

/// `POST`s to the running app's server (`dictate souffleur start`): its JSON answer, or an error
/// when Dictate is not running.
pub fn post(path: &str) -> anyhow::Result<String> {
    use anyhow::Context;
    let mut stream = TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], PORT)),
        Duration::from_secs(2),
    )
    .context("Dictate does not run (no answer on 127.0.0.1:47619)")?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{PORT}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply)?;
    let (head, body) = reply.split_once("\r\n\r\n").unwrap_or((&reply, ""));
    anyhow::ensure!(
        head.starts_with("HTTP/1.1 200"),
        "Dictate answered {head:?}"
    );
    Ok(body.to_string())
}

fn answer(status: u16, text: &str, origin: Option<&str>, private: bool, body: &str) -> String {
    let mut head = format!(
        "HTTP/1.1 {status} {text}\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nConnection: close\r\n"
    );
    if let Some(origin) = origin {
        head += &format!(
            "Access-Control-Allow-Origin: {origin}\r\nVary: Origin\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: content-type\r\nAccess-Control-Max-Age: 600\r\n"
        );
        if private {
            head += "Access-Control-Allow-Private-Network: true\r\n";
        }
    }
    format!("{head}Content-Length: {}\r\n\r\n{body}", body.len())
}

fn origin_allowed(origin: &str) -> bool {
    origins().iter().any(|o| o == origin) || loopback_origin(origin)
}

/// `http://localhost:5173`, `http://127.0.0.1:8640`: a page on this Mac itself.
fn loopback_origin(origin: &str) -> bool {
    let Some(rest) = origin.strip_prefix("http://") else {
        return false;
    };
    let host = rest.split(':').next().unwrap_or("");
    rest.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == ':')
        && matches!(host, "localhost" | "127.0.0.1")
}

fn host_allowed(host: &str) -> bool {
    host == format!("127.0.0.1:{PORT}") || host == format!("localhost:{PORT}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(method: &str, path: &str, extra: &[&str]) -> String {
        let mut s = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{PORT}\r\n");
        for e in extra {
            s += e;
            s += "\r\n";
        }
        s + "\r\n"
    }

    #[test]
    fn status_for_a_web_app_with_cors() {
        let r = respond(
            &req("GET", "/status", &["Origin: https://app.example.com"]),
            |_| false,
        );
        assert!(r.starts_with("HTTP/1.1 200"));
        assert!(r.contains("Access-Control-Allow-Origin: https://app.example.com\r\n"));
        assert!(r.contains(r#""app":"dictate""#));
    }

    #[test]
    fn other_sites_get_nothing_and_cannot_toggle() {
        let mut ran = false;
        let r = respond(
            &req("POST", "/toggle", &["Origin: https://evil.example"]),
            |_| {
                ran = true;
                true
            },
        );
        assert!(r.starts_with("HTTP/1.1 403"));
        assert!(!ran);
        assert!(!r.contains("Access-Control-Allow-Origin"));
    }

    #[test]
    fn loopback_pages_and_plain_clients_pass() {
        assert!(origin_allowed("http://localhost:5173"));
        assert!(origin_allowed("http://127.0.0.1:8640"));
        assert!(origin_allowed("tauri://localhost"));
        assert!(!origin_allowed("tauri://evil.example"));
        assert!(!origin_allowed("http://localhost.evil.example"));
        assert!(!origin_allowed("http://localhost:80/x"));
        assert!(!origin_allowed("https://app.example.com.evil.example"));
        let mut ran = false;
        let r = respond(&req("POST", "/toggle", &[]), |_| {
            ran = true;
            true
        });
        assert!(r.starts_with("HTTP/1.1 200") && ran);
        assert!(r.contains(r#""ok":true"#));
    }

    #[test]
    fn rebinding_host_is_refused() {
        let r = respond(
            "GET /status HTTP/1.1\r\nHost: evil.example:47619\r\n\r\n",
            |_| false,
        );
        assert!(r.starts_with("HTTP/1.1 403"));
        let r = respond("GET /status HTTP/1.1\r\n\r\n", |_| false);
        assert!(r.starts_with("HTTP/1.1 403"));
    }

    #[test]
    fn preflight_with_private_network() {
        let r = respond(
            &req(
                "OPTIONS",
                "/toggle",
                &[
                    "Origin: https://app.example.com",
                    "Access-Control-Request-Method: POST",
                    "Access-Control-Request-Private-Network: true",
                ],
            ),
            |_| false,
        );
        assert!(r.starts_with("HTTP/1.1 204"));
        assert!(r.contains("Access-Control-Allow-Private-Network: true"));
        assert!(r.contains("Access-Control-Allow-Headers"));
    }

    #[test]
    fn unknown_path_and_content_length() {
        let r = respond(&req("GET", "/nope", &[]), |_| false);
        assert!(r.starts_with("HTTP/1.1 404"));
        assert!(r.contains(&format!(
            "Content-Length: {}\r\n",
            r#"{"error":"not found"}"#.len()
        )));
    }

    #[test]
    fn talk_takes_a_session_and_stop_ends_it() {
        let origin = ["Origin: https://app.example.com"];
        let mut got = None;
        let r = respond(&req("POST", "/talk?session=563", &origin), |a| {
            got = Some(a);
            true
        });
        assert!(r.starts_with("HTTP/1.1 200") && r.contains(r#""ok":true"#));
        assert_eq!(got, Some(Act::Talk(563)));
        for bad in ["/talk", "/talk?session=0", "/talk?session=x"] {
            let mut ran = false;
            let r = respond(&req("POST", bad, &origin), |_| {
                ran = true;
                true
            });
            assert!(r.starts_with("HTTP/1.1 400") && !ran, "{bad}");
        }
        let mut got = None;
        let r = respond(&req("POST", "/talk/stop", &origin), |a| {
            got = Some(a);
            false
        });
        assert!(r.starts_with("HTTP/1.1 200"));
        assert_eq!(got, Some(Act::EndTalk));
        let r = respond(
            &req("POST", "/talk?session=1", &["Origin: https://evil.example"]),
            |_| panic!("an outside page started a conversation"),
        );
        assert!(r.starts_with("HTTP/1.1 403"));
    }

    #[test]
    fn souffleur_starts_and_stops() {
        for (path, start) in [("/souffleur", true), ("/souffleur/stop", false)] {
            let mut got = None;
            let r = respond(&req("POST", path, &[]), |a| {
                got = Some(a);
                true
            });
            assert!(r.starts_with("HTTP/1.1 200"), "{r}");
            assert_eq!(got, Some(Act::Souffleur(start)));
            let r = respond(
                &req("POST", path, &["Origin: https://evil.example"]),
                |_| panic!("an outside page started the Souffleur"),
            );
            assert!(r.starts_with("HTTP/1.1 403"));
        }
    }

    #[test]
    fn status_shows_the_conversation() {
        *TALK.lock().unwrap() = Some((Some(7), "speaking"));
        let r = respond(&req("GET", "/status", &[]), |_| false);
        *TALK.lock().unwrap() = None;
        assert!(
            r.contains(r#""talk":{"session":7,"phase":"speaking"}"#),
            "{r}"
        );
    }
}
