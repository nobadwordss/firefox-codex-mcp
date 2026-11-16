/* Shared, dependency-free validation. Also imported by Node tests. */
(function (root) {
  "use strict";
  const MAX_MESSAGE_BYTES = 262144;
  const methods = new Set(["snapshot", "click", "type", "scroll"]);
  function validToken(value) { return typeof value === "string" && /^[a-f0-9]{64}$/i.test(value); }
  function object(value) { return value !== null && typeof value === "object" && !Array.isArray(value); }
  function validateRequest(value) {
    if (!object(value) || typeof value.id !== "string" || !value.id || value.id.length > 128 || !methods.has(value.method) || !object(value.params)) throw new Error("Invalid bridge request");
    const p = value.params;
    if (value.method !== "snapshot" && (typeof p.snapshotId !== "string" || !p.snapshotId || p.snapshotId.length > 128)) throw new Error("A fresh snapshotId is required");
    if (["click", "type"].includes(value.method) && (!Number.isInteger(p.elementId) || p.elementId < 1 || p.elementId > 200)) throw new Error("Invalid elementId");
    if (value.method === "type" && (typeof p.text !== "string" || p.text.length > 10000)) throw new Error("Text must be at most 10000 characters");
    if (value.method === "scroll" && (!Number.isInteger(p.y) || Math.abs(p.y) > 2000)) throw new Error("Scroll y must be an integer between -2000 and 2000");
    return value;
  }
  function safePage(url) { try { return ["http:", "https:"].includes(new URL(url).protocol); } catch { return false; } }
  const api = { MAX_MESSAGE_BYTES, validToken, validateRequest, safePage };
  root.FirefoxCodexProtocol = api;
  if (typeof module !== "undefined") module.exports = api;
})(globalThis);
