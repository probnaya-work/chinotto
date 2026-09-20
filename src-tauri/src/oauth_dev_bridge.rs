//! Short-lived `127.0.0.1` listener: receives `{ nonce, credential }` after Firebase Apple sign-in
//! in a **browser** session. Dev: Vite on `localhost`. Packaged DMG: system browser on Firebase Hosting.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::Emitter;

const BRIDGE_PATH: &str = "/chinotto-oauth-bridge";
const OAUTH_PAGE_PATH: &str = "/chinotto-oauth";
const HEADER_SECRET: &str = "x-chinotto-oauth-secret";
const MAX_BODY: usize = 256 * 1024;
const LISTEN_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const BRIDGE_SUCCESS_HTML: &str = "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>Signed in</title></head><body style=\"font-family:system-ui;background:#141416;color:#d4d3ce;display:flex;align-items:center;justify-content:center;min-height:100vh;margin:0\"><p>Signed in. Close this tab and return to Chinotto.</p></body></html>";

struct HttpRequest {
    request_line: String,
    secret_header: String,
    origin: Option<String>,
    content_type: Option<String>,
    body: Vec<u8>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartOauthBridgeListenerArgs {
    secret: String,
    /// Prefer this from the frontend: one JSON string avoids nested IPC deserialization issues.
    #[serde(default)]
    loopback_page_json: Option<String>,
    #[serde(default)]
    loopback_page: Option<LoopbackPageArgs>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoopbackPageArgs {
    page_token: String,
    nonce: String,
    firebase: serde_json::Value,
}

#[derive(serde::Deserialize)]
struct BridgeBody {
    nonce: String,
    credential: serde_json::Value,
}

struct LoopbackPageState {
    page_token: String,
    unlocked: bool,
    nonce: String,
    firebase: serde_json::Value,
}

fn cors_allow_origin(origin: Option<&str>) -> String {
    const HOSTED: &[&str] = &[
        "https://chinotto.firebaseapp.com",
        "https://chinotto.web.app",
    ];
    if let Some(o) = origin {
        if HOSTED.contains(&o)
            || o.starts_with("http://localhost:")
            || o.starts_with("http://127.0.0.1:")
        {
            return o.to_string();
        }
    }
    "https://chinotto.firebaseapp.com".to_string()
}

fn cors_preflight_response(origin: Option<&str>) -> String {
    let allow = cors_allow_origin(origin);
    format!(
        "HTTP/1.1 204 No Content\r\n\
         Access-Control-Allow-Origin: {allow}\r\n\
         Access-Control-Allow-Methods: POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type, X-Chinotto-OAuth-Secret\r\n\
         Access-Control-Allow-Private-Network: true\r\n\
         Access-Control-Max-Age: 3600\r\n\
         Connection: close\r\n\r\n"
    )
}

fn respond_json(
    stream: &mut TcpStream,
    code: u16,
    reason: &str,
    body: &str,
    origin: Option<&str>,
) -> Result<(), std::io::Error> {
    let allow = cors_allow_origin(origin);
    let text = format!(
        "HTTP/1.1 {code} {reason}\r\n\
         Content-Type: application/json; charset=utf-8\r\n\
         Access-Control-Allow-Origin: {allow}\r\n\
         Access-Control-Allow-Private-Network: true\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n\
         {body}",
        body.len()
    );
    stream.write_all(text.as_bytes())
}

fn respond_html(
    stream: &mut TcpStream,
    body: &str,
    origin: Option<&str>,
) -> Result<(), std::io::Error> {
    let allow = cors_allow_origin(origin);
    let text = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Access-Control-Allow-Origin: {allow}\r\n\
         Access-Control-Allow-Private-Network: true\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n\
         {body}",
        body.len()
    );
    stream.write_all(text.as_bytes())
}

fn read_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(|e| e.to_string())?;
    let request_line = request_line.trim().to_string();

    let mut content_length = 0usize;
    let mut secret_header: Option<String> = None;
    let mut origin: Option<String> = None;
    let mut content_type: Option<String> = None;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        if line == "\r\n" || line == "\n" || line.is_empty() {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("content-length:") {
            content_length = line
                .split(':')
                .nth(1)
                .ok_or_else(|| "bad Content-Length".to_string())?
                .trim()
                .parse::<usize>()
                .map_err(|_| "bad Content-Length value".to_string())?;
        } else if lower.starts_with(HEADER_SECRET) {
            secret_header = line
                .split(':')
                .nth(1)
                .map(|s| s.trim().to_string());
        } else if lower.starts_with("origin:") {
            origin = Some(line.split_once(':').and_then(|(_, v)| Some(v.trim().to_string())).unwrap_or_default());
        } else if lower.starts_with("content-type:") {
            content_type = Some(line.split_once(':').and_then(|(_, v)| Some(v.trim().to_string())).unwrap_or_default());
        }
    }

    if content_length > MAX_BODY {
        return Err("body too large".to_string());
    }

    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).map_err(|e| e.to_string())?;

    Ok(HttpRequest {
        request_line,
        secret_header: secret_header.unwrap_or_default(),
        origin,
        content_type,
        body,
    })
}

fn url_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
        } else if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16) {
                out.push(v);
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_form_field(body: &str, key: &str) -> Option<String> {
    for pair in body.split('&') {
        let mut parts = pair.splitn(2, '=');
        let k = parts.next()?.trim();
        if k == key {
            return Some(url_decode(parts.next().unwrap_or("")));
        }
    }
    None
}

fn parse_bridge_body(content_type: Option<&str>, body: &[u8]) -> Result<(BridgeBody, Option<String>), String> {
    let ct = content_type.unwrap_or("");
    if ct.contains("application/x-www-form-urlencoded") {
        let form = std::str::from_utf8(body).map_err(|e| e.to_string())?;
        let nonce = parse_form_field(form, "nonce").ok_or_else(|| "missing nonce".to_string())?;
        let credential_raw =
            parse_form_field(form, "credential").ok_or_else(|| "missing credential".to_string())?;
        let credential: serde_json::Value =
            serde_json::from_str(&credential_raw).map_err(|e| e.to_string())?;
        let secret = parse_form_field(form, "secret");
        return Ok((BridgeBody { nonce, credential }, secret));
    }
    let parsed: BridgeBody = serde_json::from_slice(body).map_err(|e| e.to_string())?;
    Ok((parsed, None))
}

fn parse_path_query(request_target: &str) -> (&str, &str) {
    request_target
        .split_once('?')
        .unwrap_or((request_target, ""))
}

fn query_param(query: &str, key: &str) -> Option<String> {
    if query.is_empty() {
        return None;
    }
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let k = parts.next()?.trim();
        let v = parts.next().unwrap_or("").trim();
        if k == key {
            return Some(v.to_string());
        }
    }
    None
}

fn loopback_oauth_html(post_secret: &str, state: &LoopbackPageState) -> Result<String, String> {
    let cfg = serde_json::json!({
        "nonce": state.nonce,
        "postSecret": post_secret,
        "firebase": state.firebase,
    });
    let cfg_str = serde_json::to_string(&cfg).map_err(|e| e.to_string())?;
    let cfg_b64 = STANDARD.encode(cfg_str.as_bytes());
    Ok(include_str!("../oauth_loopback_page.html").replace("{{CFG_B64}}", &cfg_b64))
}

fn handle_get_oauth(
    stream: &mut TcpStream,
    query: &str,
    post_secret: &str,
    loop_state: &Arc<Mutex<Option<LoopbackPageState>>>,
) -> Result<(), std::io::Error> {
    let mut guard = match loop_state.lock() {
        Ok(g) => g,
        Err(_) => {
            let _ = stream.write_all(b"HTTP/1.1 500 Internal Server Error\r\nConnection: close\r\n\r\n");
            return Ok(());
        }
    };
    let inner = match guard.as_mut() {
        Some(s) => s,
        None => {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\n");
            return Ok(());
        }
    };

    let t = query_param(query, "t");
    if !inner.unlocked {
        match t {
            Some(ref tok) if tok == &inner.page_token => {
                inner.unlocked = true;
            }
            _ => {
                let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\n");
                return Ok(());
            }
        }
    }

    let html = match loopback_oauth_html(post_secret, inner) {
        Ok(h) => h,
        Err(_) => {
            let _ = stream.write_all(b"HTTP/1.1 500 Internal Server Error\r\nConnection: close\r\n\r\n");
            return Ok(());
        }
    };
    respond_html(stream, &html, None)
}

fn handle_connection(
    mut stream: TcpStream,
    expected_secret: &str,
    app: &tauri::AppHandle,
    loop_state: &Arc<Mutex<Option<LoopbackPageState>>>,
) -> Result<bool, String> {
    let req = read_request(&mut stream)?;
    let origin = req.origin.as_deref();

    let parts: Vec<&str> = req.request_line.split_whitespace().collect();
    if parts.len() < 2 {
        let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n");
        return Ok(false);
    }
    let method = parts[0];
    let request_target = parts[1];
    let (path, query) = parse_path_query(request_target);

    if method == "GET" && path == OAUTH_PAGE_PATH {
        let _ = handle_get_oauth(&mut stream, query, expected_secret, loop_state);
        return Ok(false);
    }

    if method == "OPTIONS" && (path == BRIDGE_PATH || path == OAUTH_PAGE_PATH) {
        let _ = stream.write_all(cors_preflight_response(origin).as_bytes());
        return Ok(false);
    }

    if method != "POST" || path != BRIDGE_PATH {
        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\n");
        return Ok(false);
    }

    let (parsed, form_secret) =
        parse_bridge_body(req.content_type.as_deref(), &req.body)?;
    let secret = if !req.secret_header.is_empty() {
        req.secret_header.as_str()
    } else if let Some(ref s) = form_secret {
        s.as_str()
    } else {
        ""
    };

    if secret != expected_secret {
        let msg = r#"{"ok":false,"error":"unauthorized"}"#;
        let _ = respond_json(&mut stream, 401, "Unauthorized", msg, origin);
        return Ok(false);
    }

    let payload = serde_json::json!({
        "nonce": parsed.nonce,
        "credential": parsed.credential,
    });

    app.emit("chinotto-sync-oauth-success", payload)
        .map_err(|e| e.to_string())?;

    let form_post = req
        .content_type
        .as_deref()
        .is_some_and(|ct| ct.contains("application/x-www-form-urlencoded"));
    if form_post {
        let _ = respond_html(&mut stream, BRIDGE_SUCCESS_HTML, origin);
    } else {
        let ok = r#"{"ok":true}"#;
        let _ = respond_json(&mut stream, 200, "OK", ok, origin);
    }
    Ok(true)
}

/// Binds `127.0.0.1:0`. Optional `loopbackPage` serves `GET /chinotto-oauth` on `http://127.0.0.1:{port}`
/// (Safari): Firebase runs on **http** so redirect + POST to the same listener work. Otherwise only
/// `POST /chinotto-oauth-bridge` (dev tab posting from `https://localhost`).
#[tauri::command]
pub fn start_oauth_dev_bridge_listener(
    app: tauri::AppHandle,
    args: StartOauthBridgeListenerArgs,
) -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    listener
        .set_nonblocking(true)
        .map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();

    let loopback = if let Some(ref raw) = args.loopback_page_json {
        if raw.trim().is_empty() {
            None
        } else {
            Some(
                serde_json::from_str::<LoopbackPageArgs>(raw)
                    .map_err(|e| format!("loopbackPageJson: {e}"))?,
            )
        }
    } else {
        args.loopback_page
    };

    let loop_state: Arc<Mutex<Option<LoopbackPageState>>> = Arc::new(Mutex::new(
        loopback.map(|lp| LoopbackPageState {
            page_token: lp.page_token,
            unlocked: false,
            nonce: lp.nonce,
            firebase: lp.firebase,
        }),
    ));

    let app = Arc::new(app);
    let secret = Arc::new(args.secret);
    let loop_state_th = Arc::clone(&loop_state);
    std::thread::spawn(move || {
        let deadline = Instant::now() + LISTEN_TIMEOUT;
        let mut done = false;
        while Instant::now() < deadline && !done {
            match listener.accept() {
                Ok((stream, _)) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
                    match handle_connection(
                        stream,
                        secret.as_str(),
                        app.as_ref(),
                        &loop_state_th,
                    ) {
                        Ok(true) => done = true,
                        Ok(false) => {}
                        Err(e) => {
                            log::warn!("[oauth_dev_bridge] request error: {e}");
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(80));
                }
                Err(e) => {
                    log::warn!("[oauth_dev_bridge] accept: {e}");
                    break;
                }
            }
        }
    });

    Ok(port)
}
