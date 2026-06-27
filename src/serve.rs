//! The `nono dev` server.
//!
//! A deliberately tiny HTTP server (std::net only, no framework) that rebuilds
//! the whole site on every request and serves it from a scratch output dir.
//! Full-rebuild-per-request suits a build-time toy: the sites are small, so it
//! is cheap, and it means you never stare at stale HTML. A failed build renders
//! in the browser instead of taking the server down, so you fix it and refresh.

use crate::build::{self, BuildConfig};
use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

pub fn serve(project: PathBuf, out: PathBuf, port: u16) -> Result<()> {
    let cfg = BuildConfig {
        project: project.clone(),
        out,
    };

    // Build once up front so the terminal shows the first result. A failure here
    // is not fatal: the server still starts and the error renders in-browser.
    match build::build(&cfg) {
        Ok(stats) => println!(
            "first build: {} pages",
            stats.nono_pages + stats.content_pages
        ),
        Err(e) => eprintln!("first build failed (serving the error page):\n{:#}", e),
    }

    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).with_context(|| format!("binding {addr}"))?;

    println!("nono dev: serving {} on http://{}", project.display(), addr);
    println!("rebuilds on every request. ctrl-c when you have had enough.");

    for stream in listener.incoming() {
        match stream {
            // One request at a time on purpose: a full rebuild rewrites the out
            // dir, and serialising requests keeps that race-free without locks.
            Ok(stream) => {
                if let Err(e) = handle(stream, &cfg) {
                    eprintln!("request error: {e:#}");
                }
            }
            Err(e) => eprintln!("connection error: {e}"),
        }
    }
    Ok(())
}

fn handle(mut stream: TcpStream, cfg: &BuildConfig) -> Result<()> {
    let (method, target) = {
        let mut reader = BufReader::new(&mut stream);
        match read_request(&mut reader)? {
            Some(r) => r,
            None => return Ok(()),
        }
    };
    if method != "GET" && method != "HEAD" {
        return write_response(
            &mut stream,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            b"only GET here, this is a toy",
        );
    }

    // Rebuild so edits show on refresh. If it fails, serve the error in-browser.
    if let Err(e) = build::build(cfg) {
        let body = error_page(&e);
        return write_response(
            &mut stream,
            "500 Internal Server Error",
            "text/html; charset=utf-8",
            body.as_bytes(),
        );
    }

    match resolve(&cfg.out, &target) {
        Some(path) if path.is_file() => {
            let bytes =
                std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
            write_response(&mut stream, "200 OK", content_type(&path), &bytes)
        }
        _ => {
            let body = not_found_page(&target);
            write_response(
                &mut stream,
                "404 Not Found",
                "text/html; charset=utf-8",
                body.as_bytes(),
            )
        }
    }
}

/// Read the request line and drain the headers; return (method, target).
fn read_request<R: BufRead>(reader: &mut R) -> Result<Option<(String, String)>> {
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(None);
    }
    // Consume the rest of the headers; we route on the path alone.
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    Ok(Some((method, target)))
}

/// Map a request path to a file in the output dir, honouring vanity URLs
/// (`/posts/x` and `/posts/x/` both resolve to `posts/x/index.html`). Returns
/// None if the path tries to climb out of the output dir.
fn resolve(out: &Path, target: &str) -> Option<PathBuf> {
    let path = target.split(['?', '#']).next().unwrap_or("");
    let trimmed = path.trim_start_matches('/');
    if trimmed.split('/').any(|c| c == "..") {
        return None;
    }
    let base = out.join(trimmed);
    let candidate = if trimmed.is_empty() || path.ends_with('/') {
        base.join("index.html")
    } else if base.extension().is_none() {
        // A vanity URL pointing at a directory: serve its index.html.
        base.join("index.html")
    } else {
        base
    };
    Some(candidate)
}

fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> Result<()> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "txt" | "md" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        _ => "application/octet-stream",
    }
}

fn error_page(e: &anyhow::Error) -> String {
    format!(
        "<!doctype html><meta charset=\"utf-8\"><title>build failed</title>\
         <body style=\"font:14px ui-monospace,monospace;padding:2rem;background:#1d1f21;color:#f0f0f0\">\
         <h1>nono build failed</h1>\
         <pre style=\"white-space:pre-wrap;color:#ff8888\">{}</pre>\
         <p style=\"color:#888\">fix it and refresh. no, no, you will do it yourself.</p>\
         </body>",
        escape(&format!("{e:#}"))
    )
}

fn not_found_page(target: &str) -> String {
    format!(
        "<!doctype html><meta charset=\"utf-8\"><title>404</title>\
         <body style=\"font:14px ui-monospace,monospace;padding:2rem\">\
         <h1>404</h1><p>nothing built to <code>{}</code>.</p></body>",
        escape(target)
    )
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
