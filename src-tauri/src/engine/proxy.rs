use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::io::Write;
use std::fs::OpenOptions;
use std::path::PathBuf;
use tokio::sync::{oneshot, RwLock};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tauri::AppHandle;
use crate::storage::block_rule::BlockRule;
use crate::engine::rule_match::{match_str, matches_url};

const BLOCK_HTML: &str = include_str!("../../assets/block.html");

/// Cap on the size of a single HTTP request line we'll read. Legitimate
/// request lines (method + absolute URL + version) are well under 8 KiB;
/// anything bigger is almost certainly a hostile client or a misbehaving
/// upstream. We bound the reader so a single connection can't push us
/// into an unbounded allocation.
const MAX_REQUEST_LINE: usize = 16 * 1024;

/// Master switch for the on-disk proxy debug log. Off by default — log
/// lines reveal which URLs the user visits (privacy) and add disk I/O on
/// every connection. Enable with `QUIETLY_PROXY_DEBUG=1` for diagnosing
/// proxy behaviour.
static PROXY_DEBUG_LOG_ENABLED: AtomicBool = AtomicBool::new(false);

/// Called once at app startup. Re-reads the env var so a user can
/// toggle the log without recompiling.
pub fn init_proxy_debug_log() {
    let enabled = std::env::var("QUIETLY_PROXY_DEBUG")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    PROXY_DEBUG_LOG_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Redact the path and query of any absolute URL embedded in `line`,
/// leaving the host (and HTTP method / version) intact. Strings with
/// no `://` are returned unchanged — they're either non-URL log lines
/// (counts, decisions) or CONNECT-style `host:port` targets which have
/// no path to redact.
fn redact_url_in_line(line: &str) -> String {
    let Some(idx) = line.find("://") else {
        return line.to_string();
    };
    let scheme_end = idx + 3;
    let after = &line[scheme_end..];
    let host_end = after
        .find(|c: char| matches!(c, '/' | '?' | ' ' | '\r' | '\n'))
        .unwrap_or(after.len());
    let host = &after[..host_end];
    let tail = &after[host_end..];
    let tail_end = tail
        .find(|c: char| matches!(c, ' ' | '\r' | '\n'))
        .unwrap_or(tail.len());
    let version = &tail[tail_end..];
    format!("{}{}/<redacted>{}", &line[..scheme_end], host, version)
}

fn dbg_proxy_log(log_path: &PathBuf, msg: &str) {
    if !PROXY_DEBUG_LOG_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let msg = redact_url_in_line(msg);
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_path) {
        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let _ = writeln!(f, "[{}] {}", ts, msg);
    }
}

pub struct ProxyHandle {
    addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    join: Option<tokio::task::JoinHandle<()>>,
}

impl ProxyHandle {
    pub fn addr(&self) -> SocketAddr { self.addr }

    pub async fn stop(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(j) = self.join.take() {
            let _ = j.await;
        }
    }

    /// Test-only: build a no-op handle so integration tests can populate
    /// `AppState.active_proxy` without binding a TCP listener. The fields
    /// are private, so callers from outside `engine/proxy.rs` need this.
    pub fn dummy_for_test() -> Self {
        Self {
            addr: "127.0.0.1:0".parse().expect("127.0.0.1:0 is a valid SocketAddr"),
            shutdown: None,
            join: None,
        }
    }
}

pub async fn start(
    _app: AppHandle,
    port: u16,
    rules: Arc<RwLock<Vec<BlockRule>>>,
    app_data_dir: Arc<PathBuf>,
) -> Result<ProxyHandle, Box<dyn std::error::Error + Send + Sync>> {
    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let local_addr = listener.local_addr()?;

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let log_path: Arc<PathBuf> = Arc::new(app_data_dir.join("proxy-debug.log"));

    let join = tokio::spawn(async move {
        run_proxy_loop(listener, rules, shutdown_rx, log_path).await;
    });

    Ok(ProxyHandle {
        addr: local_addr,
        shutdown: Some(shutdown_tx),
        join: Some(join),
    })
}

async fn run_proxy_loop(
    listener: tokio::net::TcpListener,
    rules: Arc<RwLock<Vec<BlockRule>>>,
    mut shutdown_rx: oneshot::Receiver<()>,
    log_path: Arc<PathBuf>,
) {
    loop {
        tokio::select! {
            _ = &mut shutdown_rx => break,
            accept = listener.accept() => {
                match accept {
                    Ok((stream, _peer)) => {
                        let rules_inner = rules.clone();
                        let log_inner = log_path.clone();
                        tokio::spawn(async move {
                            handle_connection(stream, rules_inner, log_inner).await;
                        });
                    }
                    Err(_) => break,
                }
            }
        }
    }
}

pub fn should_block_url(url: &str, rules: &[BlockRule]) -> bool {
    if whitelist_covers_url(url, rules) {
        return false;
    }
    rules.iter().filter(|r| r.active).any(|r| matches_url(r, url))
}

/// For HTTPS, the proxy only sees "CONNECT host:port" — the full URL is
/// never on the wire. We match in two passes:
/// 1. domain-kind rules: compare against the bare host
/// 2. url-kind rules: extract the host from the rule's pattern and
///    compare. This loses path specificity (e.g. "https://x.com/timeline"
///    will block the whole domain), but lets url rules block HTTPS at all
///    — otherwise they're silently ignored on HTTPS, which is a footgun.
pub fn should_block_host(host: &str, rules: &[BlockRule]) -> bool {
    if whitelist_covers_host(host, rules) {
        return false;
    }
    let host_no_port = host.split(':').next().unwrap_or(host);
    let host_lower = host_no_port.to_lowercase();
    rules.iter().filter(|r| r.active).any(|r| match r.kind.as_str() {
        "domain" => matches_url(r, host_no_port),
        "url" => {
            // Extract host from the url pattern; compare against request host.
            let rule_host = extract_host_for_connect(&r.pattern);
            match rule_host.as_deref() {
                Some(rh) => {
                    let rh_no_port = rh.split(':').next().unwrap_or(rh);
                    rh_no_port.to_lowercase() == host_lower
                        || host_lower.ends_with(&format!(".{}", rh_no_port.to_lowercase()))
                }
                None => false,
            }
        }
        _ => false,
    })
}

// Whitelist precedence: a matching active whitelist rule short-circuits the
// block decision. Whitelist patterns use the same match_type semantics as
// block rules, but the haystack is the request URL (or its host for HTTPS
// CONNECT). We try both URL and host so a domain-shaped pattern covers
// HTTPS too without the user having to add the scheme.
fn whitelist_covers_url(url: &str, rules: &[BlockRule]) -> bool {
    rules.iter().filter(|r| r.active).any(|r| {
        if r.kind != "whitelist" {
            return false;
        }
        match_str(url, &r.pattern, &r.match_type)
    })
}

fn whitelist_covers_host(host: &str, rules: &[BlockRule]) -> bool {
    let host_no_port = host.split(':').next().unwrap_or(host);
    rules.iter().filter(|r| r.active).any(|r| {
        if r.kind != "whitelist" {
            return false;
        }
        match_str(host_no_port, &r.pattern, &r.match_type)
    })
}

pub fn parse_connect_host(request_line: &str) -> Option<String> {
    let rest = request_line.strip_prefix("CONNECT ")?;
    let target = rest.split_whitespace().next()?;
    if target.is_empty() { None } else { Some(target.to_string()) }
}

pub fn parse_request_url(request_line: &str) -> Option<String> {
    let methods = ["GET ", "POST ", "PUT ", "DELETE ", "HEAD ", "OPTIONS ", "PATCH "];
    let (_method, rest) = methods.iter().find_map(|m| {
        request_line.strip_prefix(*m).map(|r| (*m, r))
    })?;
    let url = rest.split_whitespace().next()?;
    if url.starts_with("http://") || url.starts_with("https://") {
        Some(url.to_string())
    } else {
        None
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    rules: Arc<RwLock<Vec<BlockRule>>>,
    log_path: Arc<PathBuf>,
) {
    let mut buf: Vec<u8> = Vec::with_capacity(512);
    {
        // +1 lets us detect a line that exceeded MAX_REQUEST_LINE without
        // reading further bytes off the wire.
        let mut limited = (&mut stream).take((MAX_REQUEST_LINE + 1) as u64);
        let mut reader = BufReader::new(&mut limited);
        if reader.read_until(b'\n', &mut buf).await.is_err() {
            return;
        }
        if buf.len() > MAX_REQUEST_LINE {
            return;
        }
    }
    let request_line = match std::str::from_utf8(&buf) {
        Ok(s) => s.trim_end(),
        Err(_) => return,
    };
    let blocks = rules.read().await.clone();
    dbg_proxy_log(&log_path, &format!("[proxy] request: {}", request_line));
    dbg_proxy_log(&log_path, &format!("[proxy] active rules: {}", blocks.iter().filter(|r| r.active).count()));

    if let Some(target) = parse_connect_host(request_line) {
        dbg_proxy_log(&log_path, &format!("[proxy] CONNECT -> {}, block? {}", target, should_block_host(&target, &blocks)));
        if let Some(t) = crate::engine::app_usage::current() {
            t.update(None, Some(&target), None);
        }
        handle_connect(&target, &blocks, &mut stream).await;
    } else if let Some(url) = parse_request_url(request_line) {
        dbg_proxy_log(&log_path, &format!("[proxy] HTTP -> {}, block? {}", url, should_block_url(&url, &blocks)));
        if let Some(t) = crate::engine::app_usage::current() {
            t.update(None, Some(&url), None);
        }
        handle_http(request_line, &url, &blocks, &mut stream).await;
    } else {
        let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
    }
}

async fn handle_connect(
    target: &str,
    blocks: &[BlockRule],
    stream: &mut TcpStream,
) {
    if should_block_host(target, blocks) {
        serve_block(stream).await;
    } else {
        match TcpStream::connect(target).await {
            Ok(mut upstream) => {
                if stream.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await.is_err() {
                    return;
                }
                let _ = tokio::io::copy_bidirectional(stream, &mut upstream).await;
            }
            Err(_) => {
                let _ = stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
            }
        }
    }
}

async fn handle_http(
    request_line: &str,
    url: &str,
    blocks: &[BlockRule],
    stream: &mut TcpStream,
) {
    if should_block_url(url, blocks) {
        serve_block(stream).await;
        return;
    }
    let host = match extract_host_for_connect(url) {
        Some(h) => h,
        None => {
            let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
            return;
        }
    };
    match TcpStream::connect(&host).await {
        Ok(mut upstream) => {
            // Rewrite request line: absolute URL → relative path
            let path = extract_path_and_query(url);
            let rewritten = rewrite_request_line(request_line, &path);
            let _ = upstream.write_all(rewritten.as_bytes()).await;
            let _ = upstream.write_all(b"\r\n").await;
            // Pump both directions between client and upstream.
            // Note: copy_bidirectional already handles both directions
            // concurrently, so a single call is correct (the brief's two-call
            // pattern would deadlock / be redundant).
            let _ = tokio::io::copy_bidirectional(stream, &mut upstream).await;
        }
        Err(_) => {
            let _ = stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
        }
    }
}

/// Build the complete HTTP/1.1 403 response bytes for the block page.
/// Pure: deterministic, no I/O, no clock. Used by `serve_block` and
/// exercised by unit tests so the response shape is regression-locked.
pub fn build_block_response() -> Vec<u8> {
    let body = BLOCK_HTML;
    let header = format!(
        "HTTP/1.1 403 Forbidden\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n",
        body.len()
    );
    let mut response = Vec::with_capacity(header.len() + body.len());
    response.extend_from_slice(header.as_bytes());
    response.extend_from_slice(body.as_bytes());
    response
}

async fn serve_block(stream: &mut TcpStream) {
    use tokio::io::AsyncWriteExt;
    let _ = stream.write_all(&build_block_response()).await;
}

fn extract_host_for_connect(url: &str) -> Option<String> {
    let after_scheme = url.splitn(2, "://").nth(1)?;
    let host_part = after_scheme.split('/').next()?;
    if host_part.is_empty() { None } else { Some(host_part.to_string()) }
}

fn extract_path_and_query(url: &str) -> String {
    let after_scheme = url.splitn(2, "://").nth(1).unwrap_or(url);
    match after_scheme.find('/') {
        Some(idx) => after_scheme[idx..].to_string(),
        None => "/".to_string(),
    }
}

fn rewrite_request_line(request_line: &str, new_path: &str) -> String {
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let _old_url = parts.next().unwrap_or("");
    let version = parts.next().unwrap_or("HTTP/1.1");
    format!("{} {} {}", method, new_path, version)
}