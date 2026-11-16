"use strict";
const protocol = FirefoxCodexProtocol;
const GRANT_WAIT_MS = 5000;
const connections = new Map(), tabOwners = new Map();
let selectedId = null, draftConnectionCode = "";
// All command/permission state lives in this closure, never in the popup's selection.
function createConnection(identity) {
  const {clientId, port, code} = identity;
  const ENDPOINT = `ws://127.0.0.1:${port}`;
  let socket = null, reconnectTimer = null, handshakeTimer = null;
  let requested = false, ready = false, token = "", attempt = 0, sessionId = null;
  let enabledTab = null, pendingTab = null, generation = 0, connectionState = "Disconnected";

  function revoke() {
    const previous = enabledTab, pending = pendingTab;
    for (const tab of [previous, pending]) {
      const owner = tab && tabOwners.get(tab.id);
      if (owner?.connection === api && owner.generation === generation) tabOwners.delete(tab.id);
    }
    // Fence dispatch before disconnecting ports or settling asynchronous work.
    enabledTab = null; pendingTab = null; generation++;
    previous?.close();
    if (pending !== previous) pending?.close?.();
  }
  function status() {
    return { clientId, connectionCode: code, connectionState, ready, connected: ready, tabEnabled: enabledTab !== null, tabId: enabledTab?.id ?? null,
      nextStep: !ready ? "Use this chat's connectionCode to connect in the Firefox extension."
        : !enabledTab ? "Select this chat's connection in the Firefox toolbar, then click Enable current tab."
        : "Take a fresh snapshot before each action. Revoke tab access in the Firefox toolbar when finished." };
  }
  function disconnect() {
    requested = false;
    clearTimeout(reconnectTimer); clearTimeout(handshakeTimer);
    ready = false; sessionId = null; connectionState = "Disconnected";
    const previous = socket; socket = null;
    revoke();
    if (previous) previous.close();
  }
  function currentAttempt(started, current) {
    if (connections.get(clientId) !== api || generation !== started || socket !== current || !ready) throw new Error("Tab permission was revoked before dispatch. Enable it again.");
  }
  function currentGrant(grant, current) {
    currentAttempt(grant.generation, current);
    if (enabledTab !== grant || grant.closed) throw new Error("Tab permission was revoked before dispatch. Enable it again.");
  }

  function openGrant(tab, started, current) {
    const port = browser.tabs.connect(tab.id, { name: protocol.PORT_NAME, frameId: 0 });
    let resolveReady, rejectReady, pending = null;
    const bound = new Promise((resolve, reject) => { resolveReady = resolve; rejectReady = reject; });
    const grant = {
      id: tab.id, url: tab.url, generation: started, closed: false,
      context: { clientId, sessionId, grantId: crypto.randomUUID(), documentId: null },
      close() {
        if (grant.closed) return;
        grant.closed = true;
        clearTimeout(timer);
        rejectReady(new Error("Tab permission was revoked while enabling."));
        if (pending) {
          clearTimeout(pending.timer);
          pending.reject(new Error("Page connection closed after dispatch; action outcome may be unknown. Inspect before retrying."));
          pending = null;
        }
        port.disconnect();
      },
      call(request) {
        currentGrant(grant, current);
        if (pending) throw new Error("Page command already in flight");
        return new Promise((resolve, reject) => {
          pending = { id: request.id, resolve, reject, timer: setTimeout(() => {
            if (enabledTab === grant) revoke(); else grant.close();
            if (socket === current) current.close();
          }, 14000) };
          try {
            // No await between the final ownership check and dispatch.
            currentGrant(grant, current);
            port.postMessage({ kind: "command", id: request.id, method: request.method, params: request.params, context: grant.context });
          } catch (error) {
            clearTimeout(pending.timer); pending = null;
            reject(error);
            if (enabledTab === grant) revoke(); else grant.close();
          }
        });
      }
    };
    const timer = setTimeout(() => {
      if (pendingTab === grant) revoke(); else grant.close();
    }, GRANT_WAIT_MS);
    port.onDisconnect.addListener(() => {
      // A delayed disconnect from an older port must not revoke its replacement.
      if (pendingTab === grant || enabledTab === grant) revoke(); else grant.close();
    });
    port.onMessage.addListener(message => {
      try {
        currentAttempt(started, current);
        if (grant.closed) return;
        if (message?.kind === "hello" && pendingTab === grant && grant.context.documentId === null && protocol.validId(message.documentId)) {
          grant.context.documentId = message.documentId;
          port.postMessage({ kind: "grant", context: grant.context });
        } else if (message?.kind === "ready" && pendingTab === grant && protocol.sameContext(message.context, grant.context)) {
          clearTimeout(timer); resolveReady(grant);
        } else if (message?.kind === "response" && enabledTab === grant && pending && message.id === pending.id && protocol.sameContext(message.context, grant.context)) {
          const reply = pending; pending = null; clearTimeout(reply.timer);
          if (message.error) reply.reject(new Error(message.error)); else reply.resolve(message.result);
        } else {
          throw new Error("Unexpected page response");
        }
      } catch {
        if (pendingTab === grant || enabledTab === grant) revoke(); else grant.close();
      }
    });
    pendingTab = grant;
    return bound;
  }

  function connect() {
    if (!requested || socket) return;
    connectionState = "Connecting";
    const current = new WebSocket(ENDPOINT); socket = current;
    let inFlight = false, phase = "challenge", challenge = null, verification = null;
    function authFailed() {
      requested = false;
      clearTimeout(handshakeTimer); ready = false; sessionId = null;
      socket = null; revoke(); current.close();
      connectionState = "Pairing failed; check this chat's current code and token";
    }
    current.onopen = () => {
      if (socket !== current || !requested) { current.close(); return; }
      handshakeTimer = setTimeout(() => current.close(), 5000);
    };
    current.onmessage = async event => {
      if (socket !== current || typeof event.data !== "string") return;
      if (new TextEncoder().encode(event.data).length > protocol.MAX_MESSAGE_BYTES) { current.close(); return; }
      // A server may queue a command immediately behind its proof. Wait for the
      // asynchronous WebCrypto check before accepting any application message.
      if (phase === "verifying") {
        try { await verification; } catch { return; }
        if (socket !== current || !ready) return;
      }
      let request, ownsFlight = false;
      try {
        request = JSON.parse(event.data);
        if (!ready) {
          if (phase === "challenge" && request.type === "challenge" && request.version === 2 && request.clientId === clientId && protocol.validToken(request.challenge)) {
            phase = "signing"; challenge = request.challenge;
            const proof = await protocol.authProof(token, clientId, challenge, "extension");
            if (socket !== current || !requested) return;
            phase = "ready";
            current.send(JSON.stringify({type:"auth", clientId, proof}));
            return;
          }
          if (phase === "ready" && request.type === "ready" && request.clientId === clientId) {
            phase = "verifying";
            verification = protocol.verifyProof(token, clientId, challenge, "server", request.proof);
            const verified = await verification;
            if (socket !== current || !requested) return;
            if (!verified) { authFailed(); return; }
            clearTimeout(handshakeTimer); ready = true; sessionId = crypto.randomUUID();
            attempt = 0; connectionState = "Connected"; phase = "connected"; return;
          }
          authFailed(); return;
        }
        protocol.validateRequest(request);
        // Connection diagnostics never require a grant or read any page data.
        if (request.method === "status") {
          const { connected, tabEnabled, tabId, nextStep } = status();
          current.send(JSON.stringify({ id: request.id, result: { connected, tabEnabled, tabId, nextStep } }));
          return;
        }
        if (inFlight) throw new Error("Firefox command already in flight");
        inFlight = true; ownsFlight = true;
        const grant = enabledTab;
        if (!grant) throw new Error("No tab enabled. Use the extension toolbar to enable this tab.");
        currentGrant(grant, current);
        const tab = await browser.tabs.get(grant.id);
        currentGrant(grant, current);
        if (tab.status === "loading" || tab.url !== grant.url || tab.incognito || !protocol.safePage(tab.url)) { revoke(); throw new Error("Tab changed. Enable it again."); }
        const result = await grant.call(request);
        if (enabledTab !== grant || socket !== current || !ready) throw new Error("Tab permission changed after dispatch; action outcome may be unknown. Inspect before retrying.");
        if (current.readyState === WebSocket.OPEN) current.send(JSON.stringify({ id: request.id, result }));
      } catch (error) {
        if (!ready) { if (socket === current) authFailed(); return; }
        if (protocol.validId(request?.id) && socket === current && current.readyState === WebSocket.OPEN) current.send(JSON.stringify({ id: request.id, error: String(error.message).slice(0, 200) }));
      } finally {
        if (ownsFlight) inFlight = false;
      }
    };
    current.onerror = () => { if (socket === current) connectionState = "Connection failed"; };
    current.onclose = () => {
      if (socket !== current) return;
      clearTimeout(handshakeTimer); socket = null; ready = false; sessionId = null; revoke();
      connectionState = requested ? "Disconnected; retrying (tab access revoked)" : "Disconnected";
      if (requested) reconnectTimer = setTimeout(connect, Math.min(1000 * 2 ** Math.min(attempt++, 5), 30000));
    };
  }
  async function configure(suppliedToken) {
      // Cancel earlier enable/connect work before the first asynchronous operation.
      disconnect();
      const started = generation;
      const supplied = suppliedToken?.trim();
      const saved = supplied || token || (await browser.storage.local.get("token")).token;
      if (started !== generation || connections.get(clientId) !== api) throw new Error("Connection request was cancelled");
      if (!protocol.validToken(saved)) throw new Error("Enter a 64-character hexadecimal token generated locally.");
      await browser.storage.local.set({ token: saved.toLowerCase() });
      if (started !== generation || connections.get(clientId) !== api) throw new Error("Connection request was cancelled");
      token = saved.toLowerCase(); requested = true; connect(); return status();
  }
  async function enable() {
      if (!ready) throw new Error("Connect to the local server first.");
      revoke();
      const started = generation, current = socket;
      try {
        const [tab] = await browser.tabs.query({ active: true, currentWindow: true });
        currentAttempt(started, current);
        if (!tab || tab.incognito || tab.status === "loading" || !protocol.safePage(tab.url)) throw new Error("Only loaded, normal HTTP(S) tabs are supported.");
        const owner = tabOwners.get(tab.id);
        if (owner && owner.connection !== api) throw new Error("This tab belongs to another chat. Revoke that chat's tab access first.");
        // Reserve synchronously before injection/handshake: two pending enables cannot win.
        tabOwners.set(tab.id, {connection:api, generation:started});
        pendingTab = { id: tab.id };
        await browser.tabs.executeScript(tab.id, { file: "protocol.js", runAt: "document_idle", frameId: 0 });
        currentAttempt(started, current);
        await browser.tabs.executeScript(tab.id, { file: "observation.js", runAt: "document_idle", frameId: 0 });
        currentAttempt(started, current);
        await browser.tabs.executeScript(tab.id, { file: "page.js", runAt: "document_idle", frameId: 0 });
        currentAttempt(started, current);
        await browser.tabs.executeScript(tab.id, { file: "content.js", runAt: "document_idle", frameId: 0 });
        currentAttempt(started, current);
        const latest = await browser.tabs.get(tab.id);
        currentAttempt(started, current);
        if (latest.status === "loading" || latest.url !== tab.url || latest.incognito) throw new Error("Tab changed while enabling. Try again.");
        const grant = await openGrant(tab, started, current);
        currentAttempt(started, current);
        pendingTab = null; enabledTab = grant;
        return status();
      } catch (error) {
        if (generation === started) revoke();
        throw error;
      }
  }
  const api = {clientId, code, status, revoke, disconnect, configure, enable};
  return api;
}

function popupStatus() {
  const selected = connections.get(selectedId);
  return {...(selected?.status() ?? {connectionState:"No chat selected",ready:false,tabEnabled:false,tabId:null,nextStep:"Ask this chat to call firefox_status, then paste its connectionCode above."}), selectedId, draftConnectionCode,
    connections:[...connections.values()].map(connection => connection.status())};
}
browser.tabs.onRemoved.addListener(id => tabOwners.get(id)?.connection.revoke());
browser.tabs.onUpdated.addListener((id, changes) => {
  if (changes.status === "loading" || changes.url) tabOwners.get(id)?.connection.revoke();
});
browser.runtime.onMessage.addListener(async (message, sender) => {
  if (sender.id !== browser.runtime.id || sender.url !== browser.runtime.getURL("popup.html")) return;
  if (message?.kind === "status") return popupStatus();
  if (message?.kind === "pairingDraft") {
    if (typeof message.connectionCode !== "string" || message.connectionCode.length > 80) throw new Error("Connection code is too long.");
    // Public pairing code only. Never retain an unsubmitted token draft.
    draftConnectionCode = message.connectionCode;
    return;
  }
  if (message?.kind === "disconnectAll") {
    for (const connection of connections.values()) connection.disconnect();
    return popupStatus();
  }
  if (message?.kind === "connect") {
    const submittedDraft = draftConnectionCode;
    const identity = protocol.connectionIdentity(message.connectionCode?.trim() || connections.get(message.clientId)?.code);
    let connection = connections.get(identity.clientId);
    if (connection && connection.code !== identity.code) throw new Error("Connection identity changed. Copy the current code from this chat.");
    if (!connection) {
      if (connections.size >= 16) throw new Error("Remove an unused chat connection first (maximum 16).");
      connection = createConnection(identity); connections.set(identity.clientId, connection);
    }
    selectedId = identity.clientId;
    await connection.configure(message.token);
    if (draftConnectionCode === submittedDraft) draftConnectionCode = "";
    return popupStatus();
  }
  const connection = connections.get(message?.clientId);
  if (!connection) throw new Error("Select a chat connection first.");
  if (message.kind === "select") selectedId = connection.clientId;
  if (message.kind === "enable") await connection.enable();
  if (message.kind === "revoke") connection.revoke();
  if (message.kind === "disconnect" || message.kind === "forget") connection.disconnect();
  if (message.kind === "forget") {
    connections.delete(connection.clientId);
    if (selectedId === connection.clientId) selectedId = null;
  }
  return popupStatus();
});
