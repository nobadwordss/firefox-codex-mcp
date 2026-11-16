"use strict";
const statusElement = document.querySelector("#status");
const clients = document.querySelector("#clients");
const connectionInput = document.querySelector("#connection");
let currentId = null;
let revision = 0, busy = false, draftLoaded = false, lastError = null;
function showError(error) {
  lastError = error.message;
  statusElement.textContent = lastError;
}
async function update() {
  const started = revision;
  const state = await browser.runtime.sendMessage({ kind: "status" });
  if (started !== revision || busy) return;
  if (!draftLoaded) { connectionInput.value = state.draftConnectionCode ?? ""; draftLoaded = true; }
  currentId = state.selectedId;
  const options = [new Option("Select a chat", ""), ...state.connections.map(connection => new Option(`${connection.clientId.slice(0,8)} · ${connection.connectionState}${connection.tabEnabled ? ` · tab ${connection.tabId}` : ""}`, connection.clientId))];
  clients.replaceChildren(...options); clients.value = currentId ?? "";
  document.querySelector("#identity").textContent = state.connectionCode ?? "";
  statusElement.textContent = lastError ?? `${state.connectionState}. ${state.tabId === null ? "No tab enabled." : `Enabled tab: ${state.tabId}.`}`;
  document.querySelector("#enable").disabled = !state.ready;
  document.querySelector("#revoke").disabled = !state.tabEnabled;
  document.querySelector("#hint").textContent = state.nextStep;
  document.querySelector("#disconnect").disabled = !currentId;
  document.querySelector("#forget").disabled = !currentId;
}
connectionInput.addEventListener("input", () => {
  lastError = null; draftLoaded = true;
  browser.runtime.sendMessage({kind:"pairingDraft",connectionCode:connectionInput.value}).catch(showError);
});
document.querySelector("#token").addEventListener("input", () => { lastError = null; });
clients.addEventListener("change", async () => {
  if (!clients.value) return;
  revision++; busy = true; lastError = null;
  document.querySelector("#enable").disabled = true;
  document.querySelector("#revoke").disabled = true;
  try { await browser.runtime.sendMessage({kind:"select",clientId:clients.value}); }
  catch (error) { showError(error); }
  finally { busy = false; await update(); }
});
for (const kind of ["connect", "enable", "revoke", "disconnect", "forget", "disconnectAll"]) {
  document.querySelector(`#${kind}`).addEventListener("click", async () => {
    if (busy) return;
    revision++; busy = true; lastError = null;
    try {
      await browser.runtime.sendMessage({ kind, clientId:currentId, ...(kind === "connect" ? { token: document.querySelector("#token").value, connectionCode:document.querySelector("#connection").value } : {}) });
      if (kind === "connect") { document.querySelector("#token").value = ""; document.querySelector("#connection").value = ""; }
      busy = false; await update();
    } catch (error) { busy = false; showError(error); }
  });
}
update().catch(showError);
// Poll only while this small popup is open, to show connection progress.
const timer = setInterval(() => update().catch(() => {}), 1500);
addEventListener("unload", () => clearInterval(timer));
