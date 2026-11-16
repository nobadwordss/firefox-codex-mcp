/* Shared, dependency-free validation. Also imported by Node tests. */
(function (root) {
  "use strict";
  const MAX_MESSAGE_BYTES = 262144;
  const PORT_NAME = "firefox-codex-mcp/grant-v1";
  const methods = new Set(["status", "snapshot", "click", "type", "scroll"]);
  function validToken(value) { return typeof value === "string" && /^[a-f0-9]{64}$/i.test(value); }
  function connectionIdentity(code) {
    const match = typeof code === "string" && /^([1-9][0-9]{3,4}):([a-f0-9]{32})$/.exec(code);
    if (!match || Number(match[1]) < 1024 || Number(match[1]) > 65535) throw new Error("Paste the connectionCode from this chat's firefox_status.");
    return {clientId: match[2], port: Number(match[1]), code};
  }
  async function authProof(token, clientId, challenge, role) {
    if (!validToken(token) || !/^[a-f0-9]{32}$/.test(clientId) || !validToken(challenge) || !["extension", "server"].includes(role)) throw new Error("Invalid authentication challenge");
    const key = await root.crypto.subtle.importKey("raw", new TextEncoder().encode(token.toLowerCase()), {name:"HMAC",hash:"SHA-256"}, false, ["sign"]);
    const value = await root.crypto.subtle.sign("HMAC", key, new TextEncoder().encode(`firefox-codex-mcp/v2/${role}\n${clientId}\n${challenge}`));
    return Array.from(new Uint8Array(value), byte => byte.toString(16).padStart(2,"0")).join("");
  }
  async function verifyProof(token, clientId, challenge, role, proof) {
    if (!validToken(proof)) return false;
    const key = await root.crypto.subtle.importKey("raw", new TextEncoder().encode(token.toLowerCase()), {name:"HMAC",hash:"SHA-256"}, false, ["verify"]);
    const bytes = Uint8Array.from(proof.match(/../g), byte => Number.parseInt(byte,16));
    return root.crypto.subtle.verify("HMAC",key,bytes,new TextEncoder().encode(`firefox-codex-mcp/v2/${role}\n${clientId}\n${challenge}`));
  }
  function object(value) { return value !== null && typeof value === "object" && !Array.isArray(value); }
  function validId(value) { return typeof value === "string" && value.length > 0 && value.length <= 128; }
  function validContext(value) { return object(value) && [value.clientId, value.sessionId, value.grantId, value.documentId].every(validId); }
  function sameContext(left, right) { return validContext(left) && validContext(right) && ["clientId", "sessionId", "grantId", "documentId"].every(key => left[key] === right[key]); }
  function validateRequest(value) {
    if (!object(value) || typeof value.id !== "string" || !value.id || value.id.length > 128 || !methods.has(value.method) || !object(value.params)) throw new Error("Invalid bridge request");
    const p = value.params;
    if (value.method === "status" && Object.keys(p).length) throw new Error("Status takes no parameters");
    if (!["status", "snapshot"].includes(value.method) && (typeof p.snapshotId !== "string" || !p.snapshotId || p.snapshotId.length > 128)) throw new Error("A fresh snapshotId is required");
    if (["click", "type"].includes(value.method) && (!Number.isInteger(p.elementId) || p.elementId < 1 || p.elementId > 200)) throw new Error("Invalid elementId");
    if (value.method === "type" && (typeof p.text !== "string" || p.text.length > 10000)) throw new Error("Text must be at most 10000 characters");
    if (value.method === "scroll" && (!Number.isInteger(p.y) || Math.abs(p.y) > 2000)) throw new Error("Scroll y must be an integer between -2000 and 2000");
    return value;
  }
  function safePage(url) { try { return ["http:", "https:"].includes(new URL(url).protocol); } catch { return false; } }
  const api = { MAX_MESSAGE_BYTES, PORT_NAME, validId, validContext, sameContext, validToken, connectionIdentity, authProof, verifyProof, validateRequest, safePage };
  root.FirefoxCodexProtocol = api;
  if (typeof module !== "undefined") module.exports = api;
})(globalThis);
