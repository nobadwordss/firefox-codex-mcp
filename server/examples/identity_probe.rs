//! Read-only desktop integration probe. Never logs arguments or other metadata.
use anyhow::Result;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{self, BufRead, Read, Write};
fn main() -> Result<()> {
    let mut input = io::stdin().lock();
    loop {
        let mut line = String::new();
        if (&mut input).take(1_048_576).read_line(&mut line)? == 0 {
            break;
        }
        let request: Value = serde_json::from_str(&line)?;
        let Some(id) = request.get("id") else {
            continue;
        };
        let result = match request["method"].as_str() {
            Some("initialize") => {
                json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"identity-probe","version":"1"}})
            }
            Some("tools/list") => {
                json!({"tools":[{"name":"identity_info","description":"Read only: report hashed host-supplied chat metadata for integration testing.","inputSchema":{"type":"object","properties":{},"additionalProperties":false},"annotations":{"readOnlyHint":true,"openWorldHint":false}}]})
            }
            Some("tools/call") => {
                let meta = &request["params"]["_meta"];
                let hash = |key| {
                    meta[key]
                        .as_str()
                        .map(|s| format!("{:x}", Sha256::digest(s.as_bytes())))
                };
                let info = json!({"threadHash":hash("threadId"),"sessionHash":hash("sessionId"),"hasThreadEnv":std::env::var_os("CODEX_THREAD_ID").is_some(),"processId":std::process::id()});
                json!({"content":[{"type":"text","text":info.to_string()}]})
            }
            _ => json!({}),
        };
        println!("{}", json!({"jsonrpc":"2.0","id":id,"result":result}));
        io::stdout().flush()?;
    }
    Ok(())
}
