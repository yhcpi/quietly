use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{oneshot, RwLock};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tauri::AppHandle;
use crate::storage::block_rule::BlockRule;
use crate::engine::rule_match::matches_url;

const BLOCK_HTML: &str = include_str!("../../assets/block.html");

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
}

pub async fn start(
    _app: AppHandle,
    port: u16,
    rules: Arc<RwLock<Vec<BlockRule>>>,
) -> Result<ProxyHandle, Box<dyn std::error::Error + Send + Sync>> {
    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let local_addr = listener.local_addr()?;

    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let join = tokio::spawn(async move {
        run_proxy_loop(listener, rules, shutdown_rx).await;
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
) {
    loop {
        tokio::select! {
            _ = &mut shutdown_rx => break,
            accept = listener.accept() => {
                match accept {
                    Ok((stream, _peer)) => {
                        let rules_inner = rules.clone();
                        tokio::spawn(async move {
                            handle_connection(stream, rules_inner).await;
                        });
                    }
                    Err(_) => break,
                }
            }
        }
    }
}

pub fn should_block_url(url: &str, rules: &[BlockRule]) -> bool {
    rules.iter().filter(|r| r.active).any(|r| matches_url(r, url))
}

pub fn should_block_host(host: &str, rules: &[BlockRule]) -> bool {
    let host_no_port = host.split(':').next().unwrap_or(host);
    rules.iter().filter(|r| r.active).any(|r| matches_url(r, host_no_port))
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

async fn handle_connection(mut stream: TcpStream, rules: Arc<RwLock<Vec<BlockRule>>>) {
    let mut request_line = String::new();
    {
        let mut reader = BufReader::new(&mut stream);
        if reader.read_line(&mut request_line).await.is_err() {
            return;
        }
    }
    let request_line = request_line.trim_end();
    let blocks = rules.read().await.clone();

    if let Some(target) = parse_connect_host(request_line) {
        handle_connect(&target, &blocks, &mut stream).await;
    } else if let Some(url) = parse_request_url(request_line) {
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

async fn serve_block(stream: &mut TcpStream) {
    use tokio::io::AsyncWriteExt;
    let body = BLOCK_HTML;
    let header = format!(
        "HTTP/1.1 403 Forbidden\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes()).await;
    let _ = stream.write_all(body.as_bytes()).await;
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