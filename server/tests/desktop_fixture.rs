//! An opt-in local page for real desktop chats in the user's existing Firefox.
//! Never launches, adopts or debugs a browser. Only this page reports its own DOM.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::{
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    thread,
    time::{Duration, Instant},
};
const PAGE: &str = include_str!("fixtures/desktop.html");

fn request(stream: &mut TcpStream, host: &str, artifacts: &Path) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut first = String::new();
    (&mut reader).take(8192).read_line(&mut first)?;
    let fields: Vec<_> = first.split_whitespace().collect();
    ensure!(fields.len() == 3, "Invalid request");
    let (method, path) = (fields[0], fields[1]);
    let mut length = 0;
    let mut origin = None;
    let mut request_host = None;
    let mut total = first.len();
    loop {
        let mut line = String::new();
        let read = (&mut reader).take(8192).read_line(&mut line)?;
        total += read;
        ensure!(read > 0 && total < 8192, "Invalid headers");
        if line == "\r\n" {
            break;
        }
        let (key, value) = line.split_once(':').context("Invalid header")?;
        match key.to_ascii_lowercase().as_str() {
            "host" => request_host = Some(value.trim().to_owned()),
            "origin" => origin = Some(value.trim().to_owned()),
            "content-length" => length = value.trim().parse::<usize>()?,
            "transfer-encoding" => anyhow::bail!("Unsupported framing"),
            _ => {}
        }
    }
    ensure!(request_host.as_deref() == Some(host), "Wrong Host");
    let (status, mime, body) = if method == "GET" && ["/a", "/b", "/c"].contains(&path) {
        ("200 OK", "text/html; charset=utf-8", PAGE)
    } else if method == "POST" && path == "/evidence" {
        ensure!(
            origin.as_deref() == Some(&format!("http://{host}")) && length <= 4096,
            "Invalid report"
        );
        let mut bytes = vec![0; length];
        reader.read_exact(&mut bytes)?;
        let value: Value = serde_json::from_slice(&bytes)?;
        ensure!(
            ["/a", "/b", "/c"].contains(&value["channel"].as_str().unwrap_or("")),
            "Invalid channel"
        );
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(artifacts.join("events.jsonl"))?;
        writeln!(file, "{}", json!({"page":value}))?;
        ("200 OK", "application/json", "{\"ok\":true}")
    } else {
        ("404 Not Found", "text/plain", "Not found")
    };
    write!(stream,"HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n{body}",body.len())?;
    Ok(())
}

#[test]
#[ignore = "serves a disposable page for actual desktop chats; no browser automation"]
fn current_desktop_page() -> Result<()> {
    let artifacts = std::path::PathBuf::from(env::var("FIREFOX_DESKTOP_ARTIFACTS")?);
    fs::create_dir_all(&artifacts)?;
    ensure!(
        !artifacts.join("ready.json").exists(),
        "Use a fresh artifact directory"
    );
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let host = listener.local_addr()?.to_string();
    fs::write(
        artifacts.join("ready.json"),
        serde_json::to_vec_pretty(
            &json!({"url":format!("http://{host}"),"expiresAfterSeconds":3600}),
        )?,
    )?;
    let deadline = Instant::now() + Duration::from_secs(3600);
    while Instant::now() < deadline && !artifacts.join("finish").exists() {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = request(&mut stream, &host, &artifacts);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(25))
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
