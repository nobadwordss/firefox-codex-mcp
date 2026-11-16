//! Host metadata, not model arguments, identifies a resumable chat.
//! This isolates normal MCP clients, not hostile processes running as the same OS user.
use anyhow::{ensure, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Notify;

pub const IDLE_LIMIT: Duration = Duration::from_secs(24 * 60 * 60);
pub const MAX_CHATS: usize = 128;

pub fn host_identity(params: &Value) -> Result<Option<String>> {
    let Some(value) = params.get("_meta").and_then(|m| m.get("threadId")) else {
        return Ok(None);
    };
    let id = value.as_str().unwrap_or("");
    ensure!(
        id.len() == 36
            && id.bytes().enumerate().all(|(i, b)| {
                if [8, 13, 18, 23].contains(&i) {
                    b == b'-'
                } else {
                    b.is_ascii_hexdigit()
                }
            }),
        "Invalid host threadId metadata; refusing to infer chat identity"
    );
    Ok(Some(format!(
        "{:x}",
        Sha256::digest(id.to_ascii_lowercase().as_bytes())
    )))
}

#[derive(Clone)]
pub struct Attachment {
    pub key: String,
    pub owner: String,
    pub generation: String,
    pub cancelled: Arc<Notify>,
    pub persistent: bool,
}
struct Entry {
    attachment: Attachment,
    last_used: Instant,
    active: bool,
}
#[derive(Default)]
pub struct Chats {
    entries: HashMap<String, Entry>,
}
impl Chats {
    pub fn previous(&self, key: Option<&str>) -> Option<Attachment> {
        key.and_then(|key| self.entries.get(key))
            .map(|entry| entry.attachment.clone())
    }
    pub fn attach(&mut self, key: Option<&str>) -> Result<Attachment> {
        let persistent = key.is_some();
        let key = match key {
            Some(k) => k.to_owned(),
            None => format!("temporary:{}", crate::random_hex(16)?),
        };
        ensure!(
            self.entries.contains_key(&key) || self.entries.len() < MAX_CHATS,
            "Too many retained chats; wait for idle expiry or restart the helper to release all sessions"
        );
        let owner = match self.entries.get(&key) {
            Some(old) => {
                old.attachment.cancelled.notify_one();
                old.attachment.owner.clone()
            }
            None => crate::random_hex(16)?,
        };
        let attachment = Attachment {
            key: key.clone(),
            owner,
            generation: crate::random_hex(16)?,
            cancelled: Arc::new(Notify::new()),
            persistent,
        };
        self.entries.insert(
            key,
            Entry {
                attachment: attachment.clone(),
                last_used: Instant::now(),
                active: true,
            },
        );
        Ok(attachment)
    }
    pub fn current(&self, a: &Attachment) -> bool {
        self.entries
            .get(&a.key)
            .is_some_and(|e| e.active && e.attachment.generation == a.generation)
    }
    pub fn touch(&mut self, a: &Attachment) -> Result<()> {
        ensure!(
            self.current(a),
            "This MCP connection was replaced; reconnect before continuing"
        );
        if let Some(entry) = self.entries.get_mut(&a.key) {
            entry.last_used = Instant::now();
        }
        Ok(())
    }
    /// A late EOF must never suspend a replacement connection.
    pub fn detach(&mut self, a: &Attachment) -> bool {
        if !self.current(a) {
            return false;
        }
        if a.persistent {
            if let Some(e) = self.entries.get_mut(&a.key) {
                e.active = false;
            }
        } else {
            self.entries.remove(&a.key);
        }
        true
    }
    pub fn expire(&mut self) -> Vec<Attachment> {
        let keys: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, e)| e.last_used.elapsed() >= IDLE_LIMIT)
            .map(|(k, _)| k.clone())
            .collect();
        keys.into_iter()
            .filter_map(|k| self.entries.remove(&k))
            .map(|e| {
                e.attachment.cancelled.notify_one();
                e.attachment
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn only_host_thread_metadata_identifies_a_chat() {
        let id = "01234567-89ab-cdef-0123-456789abcdef";
        assert!(
            host_identity(&json!({"arguments":{"threadId":id},"_meta":{"sessionId":id}}))
                .unwrap()
                .is_none()
        );
        assert!(host_identity(&json!({"_meta":{"threadId":"../other"}})).is_err());
        assert_eq!(
            host_identity(&json!({"_meta":{"threadId":id}})).unwrap(),
            host_identity(&json!({"_meta":{"threadId":id.to_uppercase()}})).unwrap()
        );
    }
    #[test]
    fn same_chat_resumes_while_other_chats_and_late_eof_are_isolated() {
        let mut chats = Chats::default();
        let a = chats.attach(Some("A")).unwrap();
        let b = chats.attach(Some("B")).unwrap();
        let next = chats.attach(Some("A")).unwrap();
        assert_eq!(a.owner, next.owner);
        assert_ne!(a.generation, next.generation);
        assert_ne!(a.owner, b.owner);
        assert!(!chats.detach(&a));
        assert!(chats.current(&next));
        assert!(chats.current(&b));
        assert!(chats.detach(&next));
        assert_eq!(chats.attach(Some("A")).unwrap().owner, next.owner);
    }
    #[test]
    fn anonymous_clients_and_expired_chats_never_resume() {
        let mut chats = Chats::default();
        let anonymous = chats.attach(None).unwrap();
        assert!(chats.detach(&anonymous));
        assert_ne!(chats.attach(None).unwrap().owner, anonymous.owner);
        let a = chats.attach(Some("A")).unwrap();
        chats.entries.get_mut("A").unwrap().last_used = Instant::now() - IDLE_LIMIT;
        assert_eq!(chats.expire().len(), 1);
        assert!(!chats.current(&a));
        assert_ne!(chats.attach(Some("A")).unwrap().owner, a.owner);
    }
}
