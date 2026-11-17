/* Default transport. Only the native host can dispatch AI work. */
"use strict";
const P = FirefoxCodexProtocol;
const HOST = "firefox_codex_mcp";
const FILES = ["protocol.js", "observation.js", "page.js", "content.js"];
const owners = new Map();
// An attachment is a fenced MCP transport; an owner is a retained chat's tabs.
const attachments = new Map();
let port = null,
  connected = false,
  epoch = 0,
  profileId,
  enabled = true,
  preferred = null,
  profileCount = 0;
let status = "Checking the local helper.",
  retry = null,
  queue = Promise.resolve();
let settingsSupported = false,
  pendingSettings = null;
function revokeOwner(id) {
  const owner = owners.get(id);
  if (!owner) return;
  owner.alive = false;
  for (const tab of owner.tabs.values()) tab.page?.close();
  owners.delete(id);
}
function revokeAll() {
  for (const id of [...owners.keys()]) revokeOwner(id);
}
function disconnect() {
  epoch++;
  connected = false;
  settingsSupported = false;
  if (pendingSettings) {
    clearTimeout(pendingSettings.timer);
    pendingSettings.reject(Error("Connection ended. Open Settings again after reconnecting."));
    pendingSettings = null;
  }
  revokeAll();
  attachments.clear();
  queue = Promise.resolve();
  const previous = port;
  port = null;
  previous?.disconnect();
}
function validOwner(value) {
  return typeof value === "string" && /^[a-f0-9]{32}$/.test(value);
}
function guard(owner, started, control = false) {
  if (
    !connected ||
    epoch !== started ||
    !owner.alive ||
    owners.get(owner.id) !== owner ||
    (!control && !enabled)
  )
    throw Error(
      "Firefox access was released before dispatch. Start with a new work tab.",
    );
}
function own(id, attachment) {
  if (!validOwner(id)) throw Error("Invalid chat identity");
  if (!validOwner(attachment) || attachments.get(id) !== attachment)
    throw Error("MCP attachment ended or was replaced; reconnect before continuing");
  let owner = owners.get(id);
  if (!owner) {
    if (owners.size >= 128) throw Error("Too many chats");
    owner = { id, alive: true, tabs: new Map(), selected: null };
    owners.set(id, owner);
  }
  return owner;
}
function suspendOwner(id, attachment) {
  if (attachments.get(id) !== attachment) return;
  attachments.delete(id);
  const owner = owners.get(id);
  if (!owner) return;
  owner.alive = false;
  for (const tab of owner.tabs.values()) invalidate(tab);
}
function attachOwner(id, attachment) {
  if (!validOwner(id) || !validOwner(attachment)) throw Error("Invalid attachment");
  if (attachments.get(id) === attachment) return;
  if (!attachments.has(id) && attachments.size >= 128) throw Error("Too many chats");
  const previous = owners.get(id);
  if (previous) {
    previous.alive = false;
    for (const tab of previous.tabs.values()) invalidate(tab);
    // Never revive the old object: queued commands captured it and must fail.
    owners.set(id, { ...previous, alive: true });
  }
  attachments.set(id, attachment);
}
function selected(owner) {
  const tab = owner.tabs.get(owner.selected);
  if (!tab) throw Error("No AI work tab. Call firefox_open_tab.");
  return tab;
}
function invalidate(tab) {
  tab.generation++;
  tab.page?.close();
  tab.page = null;
}
function forget(id) {
  for (const owner of owners.values()) {
    const tab = owner.tabs.get(id);
    if (tab) {
      invalidate(tab);
      owner.tabs.delete(id);
      if (owner.selected === id) owner.selected = null;
    }
  }
}
browser.tabs.onRemoved.addListener(forget);
browser.tabs.onUpdated.addListener((id, change) => {
  if (change.status === "loading" || change.url) {
    for (const owner of owners.values()) {
      const tab = owner.tabs.get(id);
      if (tab) invalidate(tab);
    }
  }
});
// No opener adoption: a page-created popup never becomes a chat capability.
async function pageFor(owner, tab, started) {
  guard(owner, started);
  if (tab.page) return tab.page;
  const deadline = Date.now() + 10000;
  while (true) {
    const current = await browser.tabs.get(tab.id);
    guard(owner, started);
    if (
      !owner.tabs.has(tab.id) ||
      current.incognito ||
      !P.safePage(current.url)
    )
      throw Error("Only owned HTTP(S) tabs can be controlled");
    if (current.status !== "loading") break;
    if (Date.now() > deadline)
      throw Error("Page is still loading; take a new snapshot later");
    await new Promise((r) => setTimeout(r, 50));
  }
  const generation = tab.generation;
  function current() {
    guard(owner, started);
    if (owner.tabs.get(tab.id) !== tab || tab.generation !== generation)
      throw Error("Page changed; take a fresh snapshot");
  }
  await browser.scripting.executeScript({
    target: { tabId: tab.id, frameIds: [0] },
    files: FILES,
  });
  current();
  const channel = browser.tabs.connect(tab.id, {
    name: P.PORT_NAME,
    frameId: 0,
  });
  let waiting = null,
    closed = false,
    resolveReady,
    rejectReady;
  const ready = new Promise((resolve, reject) => {
    resolveReady = resolve;
    rejectReady = reject;
  });
  const context = {
    clientId: owner.id,
    sessionId: crypto.randomUUID(),
    grantId: crypto.randomUUID(),
    documentId: null,
  };
  const timer = setTimeout(() => binding.close(), 5000);
  const binding = {
    close() {
      if (closed) return;
      closed = true;
      clearTimeout(timer);
      rejectReady(Error("Page connection ended; take a fresh snapshot"));
      if (waiting) {
        clearTimeout(waiting.timer);
        waiting.reject(
          Error(
            "Page connection ended after dispatch; action outcome may be unknown. Inspect before retrying.",
          ),
        );
        waiting = null;
      }
      channel.disconnect();
      if (tab.page === binding) tab.page = null;
    },
    call(method, params) {
      current();
      if (closed || waiting) throw Error("Page connection unavailable");
      return new Promise((resolve, reject) => {
        const id = crypto.randomUUID();
        waiting = {
          id,
          resolve,
          reject,
          timer: setTimeout(() => binding.close(), 10000),
        };
        try {
          current();
          channel.postMessage({ kind: "command", id, method, params, context });
        } catch (e) {
          binding.close();
          reject(e);
        }
      });
    },
  };
  tab.page = binding;
  channel.onDisconnect.addListener(() => binding.close());
  channel.onMessage.addListener((message) => {
    try {
      current();
      if (closed) return;
      if (
        message.kind === "hello" &&
        context.documentId === null &&
        P.validId(message.documentId)
      ) {
        context.documentId = message.documentId;
        channel.postMessage({ kind: "grant", context });
      } else if (
        message.kind === "ready" &&
        P.sameContext(message.context, context)
      ) {
        clearTimeout(timer);
        resolveReady(binding);
      } else if (
        message.kind === "response" &&
        waiting &&
        message.id === waiting.id &&
        P.sameContext(message.context, context)
      ) {
        const call = waiting;
        waiting = null;
        clearTimeout(call.timer);
        if (message.error) call.reject(Error(message.error));
        else call.resolve(message.result);
      } else throw Error("Unexpected page message");
    } catch {
      binding.close();
    }
  });
  return ready;
}
async function execute(request, started) {
  const { name, args, owner: id } = request;
  if (
    !validOwner(id) ||
    !args ||
    typeof args !== "object" ||
    Array.isArray(args)
  )
    throw Error("Invalid work request");
  if (name === "firefox_status")
    return {
      connected,
      enabled,
      ownedTabs: owners.get(id)?.tabs.size || 0,
      tabEnabled: !!owners.get(id)?.selected,
      nextStep: enabled
        ? owners.get(id)?.tabs.size
          ? "Use firefox_tabs and a fresh snapshot to continue this chat's work tabs."
          : "Use firefox_open_tab to create a work tab."
        : "Firefox extension is paused. Resume it in the toolbar.",
    };
  if (name === "firefox_revoke") {
    revokeOwner(id);
    return { revoked: true, ownedTabs: 0 };
  }
  if (!enabled)
    throw Error("Firefox extension is paused. Resume it in the toolbar.");
  const owner = own(id, request.attachment);
  guard(owner, started);
  if (name === "firefox_open_tab") {
    if (!P.safePage(args.url)) throw Error("Only HTTP(S) URLs are supported");
    if (owner.tabs.size >= 8)
      throw Error("Close an owned work tab first (limit 8)");
    const created = await browser.tabs.create({ url: args.url, active: false });
    // Revoked during creation: leave the visible result, never adopt it.
    guard(owner, started);
    owner.tabs.set(created.id, { id: created.id, generation: 0, page: null });
    owner.selected = created.id;
    return { tabId: String(created.id), created: true };
  }
  if (name === "firefox_tabs") {
    const tabs = [];
    for (const id of [...owner.tabs.keys()]) {
      try {
        const tab = await browser.tabs.get(id);
        guard(owner, started);
        if (owner.tabs.has(id))
          tabs.push({
            tabId: String(id),
            title: tab.title,
            url: tab.url,
            selected: owner.selected === id,
          });
      } catch (e) {
        guard(owner, started);
        forget(id);
      }
    }
    return { tabs };
  }
  if (name === "firefox_select_tab") {
    const id = Number(args.tabId);
    if (!Number.isSafeInteger(id) || !owner.tabs.has(id))
      throw Error("This tab is not owned by this chat");
    owner.selected = id;
    return { tabId: String(id) };
  }
  const tab = selected(owner);
  if (name === "firefox_close_tab") {
    guard(owner, started);
    await browser.tabs.remove(tab.id);
    forget(tab.id);
    return { closed: true };
  }
  if (name === "firefox_navigate") {
    if (!P.safePage(args.url)) throw Error("Only HTTP(S) URLs are supported");
    invalidate(tab);
    guard(owner, started);
    await browser.tabs.update(tab.id, { url: args.url });
    guard(owner, started);
    return { navigated: true, tabId: String(tab.id) };
  }
  if (
    ![
      "firefox_snapshot",
      "firefox_click",
      "firefox_type",
      "firefox_scroll",
    ].includes(name)
  )
    throw Error("Unsupported command");
  if (name !== "firefox_snapshot" && !tab.page)
    throw Error("Stale snapshot. Take a fresh snapshot after navigation.");
  const page = await pageFor(owner, tab, started);
  guard(owner, started);
  return page.call(name.slice("firefox_".length), args);
}
function schedule() {
  clearTimeout(retry);
  retry = setTimeout(connect, 2000);
}
function settingsUrl(value) {
  if (typeof value !== "string") throw Error("Invalid Settings address from the local helper.");
  const match = /^http:\/\/127\.0\.0\.1:([1-9][0-9]{0,4})\/#([a-f0-9]{64})$/.exec(value);
  if (!match || Number(match[1]) > 65535)
    throw Error("Invalid Settings address from the local helper.");
  return value;
}
async function openSettings() {
  if (!connected) throw Error("Install the local helper once, then open Settings again.");
  if (!settingsSupported)
    throw Error("Update the local helper from a newer download to use Settings in Firefox.");
  if (pendingSettings) throw Error("Settings is already opening.");
  const started = epoch;
  const url = await new Promise((resolve, reject) => {
    const id = crypto.randomUUID();
    const timer = setTimeout(() => {
      if (pendingSettings?.id !== id) return;
      pendingSettings = null;
      reject(Error("Settings did not open. Try again or run the downloaded setup program."));
    }, 15000);
    pendingSettings = { id, resolve, reject, timer };
    try { port.postMessage({ kind: "openSettings", id }); }
    catch (error) {
      clearTimeout(timer);
      pendingSettings = null;
      reject(error);
    }
  });
  if (!connected || epoch !== started) throw Error("Connection changed. Open Settings again.");
  // This tab deliberately has no chat owner and cannot be adopted by MCP.
  await browser.tabs.create({ url: settingsUrl(url) });
}
function connect() {
  if (port || !profileId) return;
  const current = browser.runtime.connectNative(HOST);
  port = current;
  const started = ++epoch;
  current.onDisconnect.addListener(() => {
    if (port !== current) return;
    const error = current.error?.message;
    disconnect();
    status = error
      ? "Local helper not found. Run the setup program once."
      : "Connection lost. Reconnecting automatically.";
    schedule();
  });
  current.onMessage.addListener((message) => {
    if (port !== current || started !== epoch) return;
    if (message.kind === "ready") {
      connected = true;
      settingsSupported = Array.isArray(message.capabilities) && message.capabilities.includes("settings-ui-v1");
      status = "Connected. Codex can open its own work tabs.";
      return;
    }
    if (message.kind === "settingsResult") {
      if (!pendingSettings || pendingSettings.id !== message.id) return;
      const pending = pendingSettings;
      pendingSettings = null;
      clearTimeout(pending.timer);
      if (typeof message.error === "string") pending.reject(Error(message.error.slice(0, 400)));
      else pending.resolve(message.url);
      return;
    }
    if (message.kind === "configuration") {
      preferred = message.selected;
      profileCount = message.count;
      return;
    }
    if (message.kind === "revokeAll") {
      revokeAll();
      return;
    }
    if (message.kind === "dropOwner") {
      revokeOwner(message.owner);
      attachments.delete(message.owner);
      return;
    }
    if (message.kind === "suspendOwner") {
      suspendOwner(message.owner, message.attachment);
      return;
    }
    if (message.kind === "attachOwner") {
      try { attachOwner(message.owner, message.attachment); }
      catch { disconnect(); schedule(); }
      return;
    }
    if (
      message.kind !== "request" ||
      !connected ||
      typeof message.id !== "string"
    ) {
      disconnect();
      schedule();
      return;
    }
    // Register owner before queueing: revoke invalidates queued work as well.
    let owner;
    try {
      owner = own(message.owner, message.attachment);
    } catch (e) {
      current.postMessage({ kind: "response", id: message.id, error: String(e.message).slice(0,400) });
      return;
    }
    queue = queue
      .catch(() => {})
      .then(async () => {
        let reply;
        try {
          guard(
            owner,
            started,
            ["firefox_status", "firefox_revoke"].includes(message.name),
          );
          reply = { result: await execute(message, started) };
        } catch (e) {
          reply = { error: String(e.message).slice(0, 400) };
        }
        if (port === current && started === epoch)
          current.postMessage({ kind: "response", id: message.id, ...reply });
      });
  });
  current.postMessage({ role: "extension", version: 1, profileId, capabilities: ["chat-continuity-v1"] });
}
async function initialize() {
  const saved = await browser.storage.local.get([
    "nativeProfileId",
    "nativeEnabled",
  ]);
  profileId = saved.nativeProfileId || crypto.randomUUID();
  enabled = saved.nativeEnabled !== false;
  await browser.storage.local.set({ nativeProfileId: profileId });
  connect();
}
browser.runtime.onMessage.addListener(async (message, sender) => {
  if (
    sender.id !== browser.runtime.id ||
    sender.url !== browser.runtime.getURL("native/popup.html")
  )
    throw Error("Only extension controls may change connection settings");
  await initialized;
  if (message.action === "pause") {
    enabled = false;
    revokeAll();
    await browser.storage.local.set({ nativeEnabled: false });
  } else if (message.action === "resume") {
    enabled = true;
    await browser.storage.local.set({ nativeEnabled: true });
    connect();
  } else if (message.action === "choose") {
    if (!connected) throw Error("Install the local helper first.");
    revokeAll();
    port.postMessage({ kind: "selectProfile" });
  } else if (message.action === "revoke") {
    revokeAll();
  } else if (message.action === "settings") {
    await openSettings();
  } else if (message.action === "reset") {
    disconnect();
    profileId = crypto.randomUUID();
    await browser.storage.local.set({ nativeProfileId: profileId });
    connect();
  } else if (message.action !== "status") throw Error("Unknown control");
  const siteAccess = await browser.permissions.contains({
    origins: ["http://*/*", "https://*/*"],
  });
  return {
    connected,
    enabled,
    status,
    siteAccess,
    selected: preferred === profileId,
    profileCount,
    unselected: preferred === null,
    chats: owners.size,
    tabs: [...owners.values()].reduce((sum, o) => sum + o.tabs.size, 0),
  };
});
browser.permissions.onRemoved.addListener(() => {
  revokeAll();
});
browser.alarms.onAlarm.addListener(() => {
  if (!port) connect();
});
browser.alarms.create("connection", { periodInMinutes: 1 });
const initialized = initialize().catch(() => {
  status = "Could not save connection settings.";
});
