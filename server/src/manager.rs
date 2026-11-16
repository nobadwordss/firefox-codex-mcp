//! One-use local setup UI. No remote assets, cookies, telemetry, or CORS access.
use crate::{lifecycle, onboarding as o};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::{collections::HashMap, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};
const HTML: &str = include_str!("../../packaging/manager.html");

fn state() -> Result<Value> {
    let profiles = o::profiles(&o::home()?)?;
    let selected = lifecycle::installed_profile().or_else(|| o::choose(&profiles).ok());
    let codex = o::codex().is_ok();
    Ok(
        json!({"version":env!("CARGO_PKG_VERSION"),"installedVersion":lifecycle::installed_version(),"installedMode":lifecycle::installed_mode(),"codexAvailable":codex,
        "profiles":profiles.iter().map(|(p,_)|json!({"id":p,"name":p.file_name().map(|v|v.to_string_lossy()),"path":p,"selected":Some(p)==selected.as_ref()})).collect::<Vec<_>>(),
        "ready":codex}),
    )
}
fn action(request: Value) -> Result<Value> {
    match request["action"].as_str().unwrap_or("") {
        "state" => state(),
        "doctor" => o::doctor(),
        "repair" => lifecycle::repair(),
        "install" | "update" => {
            if request["mode"] != "bidi" {
                return lifecycle::install(None, false);
            }
            let list = o::profiles(&o::home()?)?;
            let selected = request["profile"]
                .as_str()
                .and_then(|path| list.iter().find(|(p, _)| p.to_str() == Some(path)))
                .context("고급 BiDi 방식에 사용할 Firefox 프로필을 선택하세요")?;
            lifecycle::install(
                Some(&selected.0.to_string_lossy()),
                request["action"] == "update",
            )
        }
        "uninstall" => lifecycle::uninstall(),
        _ => anyhow::bail!("Unknown setup action"),
    }
}
async fn respond(stream: &mut TcpStream, status: &str, mime: &str, body: &str) -> Result<()> {
    respond_bytes(stream, status, mime, body.as_bytes()).await
}
async fn respond_bytes(
    stream: &mut TcpStream,
    status: &str,
    mime: &str,
    body: &[u8],
) -> Result<()> {
    let headers=format!("HTTP/1.1 {status}\r\nContent-Type: {mime}; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'\r\n\r\n",body.len());
    stream.write_all(headers.as_bytes()).await?;
    stream.write_all(body).await?;
    Ok(())
}
async fn handle(mut stream: TcpStream, host: &str, token: &str) -> Result<bool> {
    let mut bytes = Vec::new();
    let mut scratch = [0u8; 2048];
    let header_end = loop {
        let read = timeout(Duration::from_secs(5), stream.read(&mut scratch)).await??;
        ensure!(read > 0, "Incomplete HTTP request");
        bytes.extend_from_slice(&scratch[..read]);
        ensure!(bytes.len() <= 16384, "HTTP header too large");
        if let Some(pos) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
    };
    let header = std::str::from_utf8(&bytes[..header_end])?;
    let mut lines = header.split("\r\n");
    let first = lines.next().context("Missing request")?;
    let parts: Vec<_> = first.split_whitespace().collect();
    ensure!(parts.len() == 3, "Invalid request");
    let (method, path) = (parts[0].to_owned(), parts[1].to_owned());
    let mut fields = HashMap::new();
    for line in lines.filter(|l| !l.is_empty()) {
        let (key, value) = line.split_once(':').context("Invalid header")?;
        ensure!(
            fields
                .insert(key.to_ascii_lowercase(), value.trim().to_owned())
                .is_none(),
            "Duplicate header"
        );
    }
    let origin = format!("http://{host}");
    let allowed = fields.get("host").is_some_and(|h| h == host)
        && !fields.contains_key("transfer-encoding")
        && fields.get("origin").is_none_or(|v| v == &origin)
        && fields
            .get("sec-fetch-site")
            .is_none_or(|v| v == "same-origin" || v == "none");
    if !allowed {
        respond(&mut stream, "403 Forbidden", "text/plain", "Forbidden").await?;
        return Ok(false);
    }
    if method == "GET" && path == "/extension.xpi" {
        respond_bytes(
            &mut stream,
            "200 OK",
            "application/x-xpinstall",
            include_bytes!("../../packaging/signed/codex-work-tabs-0.4.0.xpi"),
        )
        .await?;
        return Ok(false);
    }
    if method == "GET" && path == "/" {
        respond(&mut stream, "200 OK", "text/html", HTML).await?;
        return Ok(false);
    }
    if method != "POST"
        || path != "/api"
        || fields.get("authorization") != Some(&format!("Bearer {token}"))
        || fields
            .get("content-type")
            .is_none_or(|v| !v.starts_with("application/json"))
    {
        respond(&mut stream, "403 Forbidden", "text/plain", "Forbidden").await?;
        return Ok(false);
    }
    let length = fields
        .get("content-length")
        .context("Missing length")?
        .parse::<usize>()?;
    ensure!(length <= 4096, "Request too large");
    while bytes.len() < header_end + length {
        let read = timeout(Duration::from_secs(5), stream.read(&mut scratch)).await??;
        ensure!(read > 0, "Incomplete body");
        bytes.extend_from_slice(&scratch[..read]);
    }
    let request: Value = serde_json::from_slice(&bytes[header_end..header_end + length])?;
    if request["action"] == "close" {
        respond(
            &mut stream,
            "200 OK",
            "application/json",
            "{\"ok\":true,\"result\":{\"nextStep\":\"관리 화면을 닫아도 됩니다.\"}}",
        )
        .await?;
        return Ok(true);
    }
    let result = tokio::task::spawn_blocking(move || action(request)).await?;
    let reply = match result {
        Ok(value) => json!({"ok":true,"result":value}),
        Err(e) => json!({"ok":false,"error":format!("{e:#}")}),
    };
    respond(
        &mut stream,
        "200 OK",
        "application/json",
        &reply.to_string(),
    )
    .await?;
    Ok(false)
}
pub async fn serve(no_open: bool) -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let host = listener.local_addr()?.to_string();
    let token = crate::random_hex(32)?;
    let url = format!("http://{host}/#{token}");
    println!("Firefox Codex 관리: {url}");
    if !no_open {
        let _ = std::process::Command::new(
            std::env::var_os("FIREFOX_BINARY").unwrap_or_else(|| "/usr/bin/firefox".into()),
        )
        .arg(&url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1200);
    while let Ok(Ok((stream, _))) = tokio::time::timeout_at(deadline, listener.accept()).await {
        match handle(stream, &host, &token).await {
            Ok(true) => break,
            Ok(false) => {}
            Err(_) => { /* Drop malformed requests without revealing internals. */ }
        }
    }
    Ok(())
}
