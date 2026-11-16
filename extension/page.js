/* Shared observation/action engine, independent of extension or BiDi transport. */
(() => {
  "use strict";
  if (globalThis.FirefoxCodexPage) return;
  globalThis.FirefoxCodexPage = function createPage() {
  const protocol = FirefoxCodexProtocol;
  const observation = FirefoxCodexObservation;
  const SNAPSHOT_TTL_MS = 30000;
  const documentId = crypto.randomUUID();
  let revision = 0, viewportRevision = 0, snapshot = null, context = null, pageActive = true;
  function revoke() { context = null; snapshot = null; }
  addEventListener("pagehide", () => { pageActive = false; revoke(); });
  // BFCache restoration does not restore an application grant.
  addEventListener("pageshow", () => { pageActive = true; });
  const observer = new MutationObserver(() => { revision++; });
  observer.observe(document, { subtree: true, childList: true, attributes: true, characterData: true });
  addEventListener("scroll", () => { revision++; viewportRevision++; }, true);
  addEventListener("resize", () => { revision++; viewportRevision++; });
  addEventListener("input", () => { revision++; }, true);
  function syncMutations() { if (observer.takeRecords().length) revision++; }
  const visible = observation.visible;
  function sensitive(element) {
    return element instanceof HTMLInputElement && (["password", "file", "hidden"].includes(element.type) || /password|one-time-code|cc-|transaction-/i.test(element.autocomplete));
  }
  function viewport() { return {width: innerWidth, height: innerHeight, scrollX, scrollY}; }
  function capture() {
    syncMutations();
    const elements = new Map(), descriptions = [], contextCache = new Map();
    const candidates = document.querySelectorAll('a[href],button,input,textarea,select,[role="button"],[role="link"],[tabindex]');
    let scanned = 0;
    for (const element of candidates) {
      if (++scanned > 2000 || descriptions.length >= 200) break;
      if (sensitive(element) || !visible(element)) continue;
      const id = descriptions.length + 1;
      elements.set(id, {element, fingerprint: observation.fingerprint(element, contextCache)});
      descriptions.push(observation.describe(element, id));
    }
    const body = observation.text(document.body), regions = observation.regions(), modals = observation.regions(observation.MODALS);
    const headings = [];
    let headingTruncated = false, scannedHeadings = 0;
    for (const element of document.querySelectorAll('h1,h2,h3,h4,h5,h6,[role="heading"]')) {
      if (++scannedHeadings > 2000 || headings.length >= observation.LIMITS.headings) {headingTruncated = true; break;}
      if (visible(element)) {
        const content = observation.text(element, 500);
        headings.push({level: Number(element.getAttribute('aria-level') || element.tagName.slice(1)) || null, text: content.text});
        headingTruncated ||= content.truncated;
      }
    }
    const snapshotId = crypto.randomUUID();
    snapshot = {snapshotId, viewportRevision, viewport: viewport(), capturedAt: performance.now(), url: location.href, elements, context: context, regions, modals};
    const truncation = {elements: scanned > 2000 || descriptions.length >= 200, text: body.truncated, headings: headingTruncated, statuses: regions.truncated};
    return {snapshotId, context: context, title: document.title.slice(0,200), url: location.href.slice(0,2048), readyState: document.readyState, viewport: viewport(), observation: {revision, maxAgeMs: SNAPSHOT_TTL_MS}, text: body.text, headings, statuses: regions.items.map(({node, ...item}) => item), elements: descriptions, truncation, truncated: Object.values(truncation).some(Boolean)};
  }
  function fresh(params) {
    syncMutations();
    if (!snapshot || !protocol.sameContext(snapshot.context, context) || params.snapshotId !== snapshot.snapshotId || location.href !== snapshot.url || performance.now() - snapshot.capturedAt > SNAPSHOT_TTL_MS || viewportRevision !== snapshot.viewportRevision || JSON.stringify(viewport()) !== JSON.stringify(snapshot.viewport)) throw new Error("Stale snapshot. Take a new snapshot before acting.");
    const regions = observation.regions(), modals = observation.regions(observation.MODALS);
    if (regions.truncated || modals.truncated || observation.regionKey(regions) !== observation.regionKey(snapshot.regions) || observation.regionKey(modals) !== observation.regionKey(snapshot.modals) || modals.items.some((item, i) => item.node !== snapshot.modals.items[i]?.node)) throw new Error("Stale snapshot: page status or dialog changed. Take a new snapshot.");
    return snapshot;
  }
  function target(params) {
    const entry = fresh(params).elements.get(params.elementId), element = entry?.element;
    if (!element || !element.isConnected || sensitive(element) || observation.disabled(element) || observation.busy(element) || !visible(element) || !observation.hitVisible(element)) throw new Error("Element is no longer available or is covered, disabled, or busy");
    if (!observation.sameTarget(entry.fingerprint, element)) throw new Error("Stale snapshot: target or nearby context changed. Take a new snapshot.");
    return element;
  }
  function execute(request) {
    let actionStarted = false;
    try {
      if (!pageActive || !context) throw new Error("Page access ended. Take a new snapshot in an owned tab.");
      protocol.validateRequest(request);
      const params = request.params || {};
      let result;
      if (request.method === "snapshot") result = capture();
      else if (request.method === "click") {
        const element = target(params);
        if (element.tagName === 'SELECT') throw new Error("Select option changes are not supported");
        const anchor = element.closest("a[href]");
        if (anchor && (!["http:", "https:"].includes(new URL(anchor.href).protocol) || anchor.hasAttribute("download"))) throw new Error("Non-web links and downloads are not supported");
        snapshot = null; actionStarted = true; element.click(); result = { clicked: true };
      } else if (request.method === "type") {
        const element = target(params);
        const input = element instanceof HTMLInputElement, textarea = element instanceof HTMLTextAreaElement;
        if ((!input && !textarea) || element.readOnly || (input && !["text", "search", "email", "url", "tel", "number"].includes(element.type)) || typeof params.text !== "string" || params.text.length > 10000) throw new Error("Only non-sensitive text inputs and textareas are supported");
        snapshot = null;
        const prototype = textarea ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
        actionStarted = true;
        Object.getOwnPropertyDescriptor(prototype, "value").set.call(element, params.text);
        element.dispatchEvent(new Event("input", { bubbles: true }));
        element.dispatchEvent(new Event("change", { bubbles: true }));
        result = { typed: true };
      } else if (request.method === "scroll") {
        fresh(params);
        if (!Number.isInteger(params.y) || Math.abs(params.y) > 2000) throw new Error("Invalid scroll delta");
        snapshot = null; actionStarted = true; window.scrollBy({ left: 0, top: params.y, behavior: "instant" }); result = { scrolled: true };
      } else throw new Error("Unsupported command");
      return { result };
    } catch (error) { return { error: (actionStarted ? "Action outcome may be unknown. Inspect before retrying. " : "") + String(error.message).slice(0, 140) }; }
  }
  return {documentId, execute, revoke, bind(value) {
    if (!pageActive || !protocol.validContext(value) || value.documentId !== documentId) throw new Error("Invalid page binding");
    context = {...value}; snapshot = null;
  }};
  };
})();
