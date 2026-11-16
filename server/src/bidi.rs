//! Firefox WebDriver BiDi transport. Never exposes arbitrary protocol/script tools.
use anyhow::{bail, ensure, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashSet;
use tokio::{net::TcpStream, time::timeout};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{protocol::WebSocketConfig, Message},
    MaybeTlsStream, WebSocketStream,
};

pub fn endpoint(value: &str) -> Result<url::Url> {
    let url = url::Url::parse(value)?;
    ensure!(
        url.scheme() == "ws"
            && url.host_str() == Some("127.0.0.1")
            && url.port().is_some_and(|port| port >= 1024)
            && url.path() == "/session"
            && url.query().is_none()
            && url.fragment().is_none()
            && url.username().is_empty()
            && url.password().is_none(),
        "Firefox debug URL must be ws://127.0.0.1:PORT/session"
    );
    Ok(url)
}
pub fn page_url(value: &str) -> Result<()> {
    ensure!(value.len() <= 4096, "Page URL is too long");
    let url = url::Url::parse(value)?;
    ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.has_host()
            && url.username().is_empty()
            && url.password().is_none(),
        "Only HTTP(S) page URLs without embedded credentials are supported"
    );
    Ok(())
}

pub struct Browser {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    sequence: u64,
    pub alive: bool,
    destroyed: HashSet<String>,
    created: Vec<(String, String)>,
    sandbox: String,
}
impl Browser {
    async fn socket(address: &str) -> Result<Self> {
        let config = WebSocketConfig::default()
            .max_message_size(Some(crate::LIMIT))
            .max_frame_size(Some(crate::LIMIT));
        let (socket, _) = timeout(
            std::time::Duration::from_secs(3),
            connect_async_with_config(address, Some(config), true),
        )
        .await
        .context("Firefox debug connection timed out")?
        .context(
            "Firefox debugging is not running. Start Firefox with --remote-debugging-port 9222",
        )?;
        let browser = Self {
            socket,
            sequence: 0,
            alive: true,
            destroyed: HashSet::new(),
            created: Vec::new(),
            sandbox: format!("firefox-codex-{}", crate::random_hex(16)?),
        };
        Ok(browser)
    }
    pub async fn connect(address: &str) -> Result<Self> {
        endpoint(address)?;
        let mut browser = Self::socket(address).await?;
        // Do not silently accept dialogs, TLS errors, or request system access.
        browser
            .call(
                "session.new",
                json!({"capabilities":{"alwaysMatch":{
                    "acceptInsecureCerts":false,"unhandledPromptBehavior":{"default":"ignore"}
                }}}),
            )
            .await.context("Firefox debugging session unavailable. Another debugger or an older unrecorded session may own it; do not take over unknown sessions")?;
        if let Err(error) = browser
            .call(
                "session.subscribe",
                json!({"events":["browsingContext.contextDestroyed","browsingContext.contextCreated"]}),
            )
            .await {
            browser.end().await;
            return Err(error);
        }
        Ok(browser)
    }
    pub fn take_destroyed(&mut self) -> HashSet<String> {
        std::mem::take(&mut self.destroyed)
    }
    pub fn take_created(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.created)
    }
    pub fn was_destroyed(&self, id: &str) -> bool {
        self.destroyed.contains(id)
    }
    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        ensure!(
            self.alive,
            "Debug connection ended; tab ownership has been released"
        );
        self.sequence += 1;
        let id = self.sequence;
        let operation = async {
            self.socket
                .send(Message::Text(
                    json!({"id":id,"method":method,"params":params})
                        .to_string()
                        .into(),
                ))
                .await?;
            while let Some(message) = self.socket.next().await {
                match message? {
                    Message::Text(text) => {
                        let message: Value = serde_json::from_str(&text)?;
                        if message["method"] == "browsingContext.contextDestroyed" {
                            if let Some(context) = message["params"]["context"].as_str() {
                                if self.destroyed.len() < 1024 {
                                    self.destroyed.insert(context.to_owned());
                                }
                            }
                        }
                        if message["method"] == "browsingContext.contextCreated"
                            && message["params"]["parent"].is_null()
                        {
                            if let (Some(id), Some(opener)) = (
                                message["params"]["context"].as_str(),
                                message["params"]["originalOpener"].as_str(),
                            ) {
                                if self.created.len() < 1024 {
                                    self.created.push((id.to_owned(), opener.to_owned()));
                                }
                            }
                        }
                        if message["id"] == id {
                            return Ok::<Value, anyhow::Error>(message);
                        }
                    }
                    Message::Ping(bytes) => self.socket.send(Message::Pong(bytes)).await?,
                    Message::Close(_) => bail!("Firefox debug connection closed"),
                    _ => {}
                }
            }
            bail!("Firefox debug connection ended")
        };
        let reply = match timeout(crate::WAIT, operation).await {
            Ok(Ok(reply)) => reply,
            other => {
                self.alive = false;
                return Err(match other {
                    Ok(Err(error)) => error.context("Action outcome may be unknown. Inspect before retrying"),
                    _ => anyhow::anyhow!("Firefox timed out; action outcome may be unknown. Ownership is released; do not retry blindly"),
                });
            }
        };
        if reply["type"] == "error" {
            bail!(
                "Firefox {}: {}",
                reply["error"].as_str().unwrap_or("error"),
                reply["message"]
                    .as_str()
                    .unwrap_or("command failed")
                    .chars()
                    .take(300)
                    .collect::<String>()
            );
        }
        ensure!(
            reply["type"] == "success",
            "Unexpected Firefox protocol response"
        );
        Ok(reply["result"].clone())
    }
    pub async fn tree(&mut self, context: &str) -> Result<Value> {
        let tree = self
            .call(
                "browsingContext.getTree",
                json!({"root":context,"maxDepth":0}),
            )
            .await?;
        let tab = tree["contexts"]
            .as_array()
            .and_then(|items| items.first())
            .context("Owned tab was closed")?;
        ensure!(
            tab["context"] == context && tab["parent"].is_null(),
            "Unexpected browsing context"
        );
        Ok(json!({"tabId":context,"url":tab["url"]}))
    }
    pub async fn observe_or_act(
        &mut self,
        context: &str,
        client_id: &str,
        method: &str,
        args: Value,
    ) -> Result<Value> {
        let tab = self.tree(context).await?;
        page_url(tab["url"].as_str().context("Missing tab URL")?)?;
        let function = format!(
            r#"function(payload) {{
            "use strict";
            if (!['http:', 'https:'].includes(location.protocol)) throw new Error('Only HTTP(S) pages can be controlled');
            {protocol}
            {observation}
            {page}
            const data = JSON.parse(payload);
            const controller = globalThis.__firefoxCodexBidiPage ||= FirefoxCodexPage();
            if (data.method === 'snapshot') controller.bind({{
                clientId:data.clientId, sessionId:data.clientId, grantId:data.clientId, documentId:controller.documentId
            }});
            return JSON.stringify(controller.execute({{id:'bidi',method:data.method,params:data.params}}));
        }}"#,
            protocol = include_str!("../../extension/protocol.js"),
            observation = include_str!("../../extension/observation.js"),
            page = include_str!("../../extension/page.js")
        );
        let reply = self.call("script.callFunction", json!({
            "functionDeclaration":function, "target":{"context":context,"sandbox":self.sandbox},
            "arguments":[{"type":"string","value":json!({"clientId":client_id,"method":method,"params":args}).to_string()}],
            "awaitPromise":false,"resultOwnership":"none","userActivation":method == "click" || method == "type"
        })).await?;
        ensure!(
            reply["type"] == "success",
            "Page script failed: {}",
            reply["exceptionDetails"]["text"]
                .as_str()
                .unwrap_or("document changed; take a fresh snapshot")
        );
        let text = reply["result"]["value"]
            .as_str()
            .context("Invalid observation response")?;
        let result: Value = serde_json::from_str(text)?;
        if let Some(error) = result["error"].as_str() {
            bail!("{error}");
        }
        Ok(result["result"].clone())
    }
    pub async fn end(&mut self) {
        if self.alive {
            let _ = self.call("session.end", json!({})).await;
        }
        let _ = self.socket.close(None).await;
        self.alive = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoints_are_local_and_pages_exclude_privileged_schemes() {
        assert!(endpoint("ws://127.0.0.1:9222/session").is_ok());
        for value in [
            "ws://example.org:9222/session",
            "ws://127.0.0.1:80/session",
            "ws://127.0.0.1:9222/session?x=1",
            "ws://user@127.0.0.1:9222/session",
        ] {
            assert!(endpoint(value).is_err());
        }
        assert!(page_url("https://example.org/a").is_ok());
        for value in [
            "file:///etc/passwd",
            "about:config",
            "javascript:alert(1)",
            "https://user:secret@example.org/",
        ] {
            assert!(page_url(value).is_err());
        }
    }
}
