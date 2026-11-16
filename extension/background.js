"use strict";
const protocol = FirefoxCodexProtocol;
const ENDPOINT = "ws://127.0.0.1:8765";
let socket = null, reconnectTimer = null, handshakeTimer = null;
let requested = false, ready = false, token = "", attempt = 0;
let enabledTab = null, pendingTab = null, generation = 0, connectionState = "Disconnected";

function revoke() { enabledTab = null; pendingTab = null; generation++; }
function status() { return { connectionState, ready, tabId: enabledTab?.id ?? null }; }
function disconnect() {
  requested = false;
  clearTimeout(reconnectTimer); clearTimeout(handshakeTimer);
  revoke(); ready = false; connectionState = "Disconnected";
  const previous = socket; socket = null;
  if (previous) previous.close();
}
function connect() {
  if (!requested || socket) return;
  connectionState = "Connecting";
  const current = new WebSocket(ENDPOINT); socket = current;
  current.onopen = () => {
    current.send(JSON.stringify({ type: "auth", token }));
    handshakeTimer = setTimeout(() => current.close(), 5000);
  };
  current.onmessage = async event => {
    if (socket !== current || typeof event.data !== "string") return;
    if (new TextEncoder().encode(event.data).length > protocol.MAX_MESSAGE_BYTES) { current.close(); return; }
    let request;
    try {
      request = JSON.parse(event.data);
      if (!ready) {
        if (request.type !== "ready") { current.close(); return; }
        clearTimeout(handshakeTimer); ready = true; attempt = 0; connectionState = "Connected"; return;
      }
      protocol.validateRequest(request);
      const grant = enabledTab, started = generation;
      if (!grant) throw new Error("No tab enabled. Use the extension toolbar to enable this tab.");
      const tab = await browser.tabs.get(grant.id);
      if (tab.url !== grant.url || tab.incognito || !protocol.safePage(tab.url)) { revoke(); throw new Error("Tab changed. Enable it again."); }
      const result = await browser.tabs.sendMessage(grant.id, { kind: "command", ...request }, { frameId: 0 });
      if (generation !== started || enabledTab?.id !== grant.id) throw new Error("Tab permission was revoked during this request");
      if (!result || result.error) throw new Error(result?.error || "Missing page response");
      if (socket === current && current.readyState === WebSocket.OPEN) current.send(JSON.stringify({ id: request.id, result: result.result }));
    } catch (error) {
      if (typeof request?.id === "string" && request.id.length <= 128 && socket === current && current.readyState === WebSocket.OPEN) current.send(JSON.stringify({ id: request.id, error: String(error.message).slice(0, 200) }));
    }
  };
  current.onerror = () => { connectionState = "Connection failed"; };
  current.onclose = () => {
    if (socket !== current) return;
    clearTimeout(handshakeTimer); socket = null; ready = false; revoke();
    connectionState = requested ? "Disconnected; retrying (tab access revoked)" : "Disconnected";
    if (requested) reconnectTimer = setTimeout(connect, Math.min(1000 * 2 ** Math.min(attempt++, 5), 30000));
  };
}
browser.tabs.onRemoved.addListener(id => { if (enabledTab?.id === id || pendingTab === id) revoke(); });
browser.tabs.onUpdated.addListener((id, changes) => {
  if ((enabledTab?.id === id || pendingTab === id) && (changes.status === "loading" || changes.url)) revoke();
});
browser.runtime.onMessage.addListener(async (message, sender) => {
  // Only our own popup can configure credentials or grant access. Web pages cannot.
  if (sender.id !== browser.runtime.id || sender.url !== browser.runtime.getURL("popup.html")) return;
  if (message?.kind === "status") return status();
  if (message?.kind === "disconnect") { disconnect(); return status(); }
  if (message?.kind === "connect") {
    const supplied = message.token?.trim();
    const saved = supplied || (await browser.storage.local.get("token")).token;
    if (!protocol.validToken(saved)) throw new Error("Enter a 64-character hexadecimal token generated locally.");
    disconnect(); token = saved;
    await browser.storage.local.set({ token });
    requested = true; connect(); return status();
  }
  if (message?.kind === "enable") {
    if (!ready) throw new Error("Connect to the local server first.");
    const [tab] = await browser.tabs.query({ active: true, currentWindow: true });
    if (!tab || tab.incognito || !protocol.safePage(tab.url)) throw new Error("Only normal HTTP(S) tabs are supported.");
    revoke();
    const started = generation;
    pendingTab = tab.id;
    await browser.tabs.executeScript(tab.id, { file: "content.js", runAt: "document_idle", frameId: 0 });
    const current = await browser.tabs.get(tab.id);
    if (started !== generation || !ready || current.status === "loading" || current.url !== tab.url) throw new Error("Tab changed while enabling. Try again.");
    pendingTab = null;
    enabledTab = { id: tab.id, url: tab.url };
    return status();
  }
});
