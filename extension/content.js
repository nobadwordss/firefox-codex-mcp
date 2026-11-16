(() => {
  "use strict";
  if (globalThis.__firefoxCodexMcpLoaded) return;
  globalThis.__firefoxCodexMcpLoaded = true;
  let revision = 0, snapshot = null;
  const observer = new MutationObserver(() => { revision++; });
  observer.observe(document, { subtree: true, childList: true, attributes: true, characterData: true });
  addEventListener("scroll", () => { revision++; }, true);
  addEventListener("resize", () => { revision++; });
  addEventListener("input", () => { revision++; }, true);
  function syncMutations() { if (observer.takeRecords().length) revision++; }
  function visible(element) {
    const r = element.getBoundingClientRect(), style = getComputedStyle(element);
    return r.width > 0 && r.height > 0 && r.bottom > 0 && r.right > 0 && r.top < innerHeight && r.left < innerWidth && style.visibility === "visible" && style.display !== "none" && style.opacity !== "0" && !element.closest("[inert]");
  }
  function sensitive(element) {
    return element instanceof HTMLInputElement && (["password", "file", "hidden"].includes(element.type) || /password|one-time-code|cc-|transaction-/i.test(element.autocomplete));
  }
  function describe(element, id) {
    const label = element.getAttribute("aria-label") || element.labels?.[0]?.textContent || element.getAttribute("title") || element.getAttribute("placeholder") || (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement ? "" : element.textContent) || "";
    return { id, tag: element.tagName.toLowerCase(), role: element.getAttribute("role"), type: element instanceof HTMLInputElement ? element.type : undefined, label: label.trim().replace(/\s+/g, " ").slice(0, 160) };
  }
  function capture() {
    syncMutations();
    const elements = new Map(), descriptions = [];
    const candidates = document.querySelectorAll('a[href],button,input,textarea,select,[role="button"],[role="link"],[tabindex]');
    let scanned = 0;
    for (const element of candidates) {
      if (++scanned > 2000 || descriptions.length >= 200) break;
      if (sensitive(element) || element.disabled || !visible(element)) continue;
      const id = descriptions.length + 1;
      elements.set(id, element); descriptions.push(describe(element, id));
    }
    const snapshotId = crypto.randomUUID();
    snapshot = { snapshotId, revision, url: location.href, elements };
    return { snapshotId, title: document.title.slice(0, 200), url: location.href.slice(0, 2048), viewport: { width: innerWidth, height: innerHeight }, elements: descriptions, truncated: scanned > 2000 || descriptions.length >= 200 };
  }
  function fresh(params) {
    syncMutations();
    if (!snapshot || params.snapshotId !== snapshot.snapshotId || revision !== snapshot.revision || location.href !== snapshot.url) throw new Error("Stale snapshot. Take a new snapshot before acting.");
    return snapshot;
  }
  function target(params) {
    const element = fresh(params).elements.get(params.elementId);
    if (!element || !element.isConnected || sensitive(element) || element.disabled || !visible(element)) throw new Error("Element is no longer available");
    return element;
  }
  browser.runtime.onMessage.addListener((request, sender) => {
    if (sender.id !== browser.runtime.id || request?.kind !== "command") return;
    try {
      const params = request.params || {};
      let result;
      if (request.method === "snapshot") result = capture();
      else if (request.method === "click") {
        const element = target(params);
        const anchor = element.closest("a[href]");
        if (anchor && (!["http:", "https:"].includes(new URL(anchor.href).protocol) || anchor.hasAttribute("download"))) throw new Error("Non-web links and downloads are not supported");
        snapshot = null; element.click(); result = { clicked: true };
      } else if (request.method === "type") {
        const element = target(params);
        const input = element instanceof HTMLInputElement, textarea = element instanceof HTMLTextAreaElement;
        if ((!input && !textarea) || element.readOnly || (input && !["text", "search", "email", "url", "tel", "number"].includes(element.type)) || typeof params.text !== "string" || params.text.length > 10000) throw new Error("Only non-sensitive text inputs and textareas are supported");
        snapshot = null;
        const prototype = textarea ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
        Object.getOwnPropertyDescriptor(prototype, "value").set.call(element, params.text);
        element.dispatchEvent(new Event("input", { bubbles: true }));
        element.dispatchEvent(new Event("change", { bubbles: true }));
        result = { typed: true };
      } else if (request.method === "scroll") {
        fresh(params);
        if (!Number.isInteger(params.y) || Math.abs(params.y) > 2000) throw new Error("Invalid scroll delta");
        snapshot = null; window.scrollBy({ left: 0, top: params.y, behavior: "instant" }); result = { scrolled: true };
      } else throw new Error("Unsupported command");
      return Promise.resolve({ result });
    } catch (error) { return Promise.resolve({ error: String(error.message).slice(0, 200) }); }
  });
})();
