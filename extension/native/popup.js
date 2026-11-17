"use strict";
let state;
async function send(action) {
  try {
    state = await browser.runtime.sendMessage({ action });
    document.querySelector("#permissions").hidden = state.siteAccess;
    document.querySelector("#status").textContent = state.enabled
      ? state.status
      : "Paused. All work-tab access has been revoked.";
    document.querySelector("#profile").textContent = !state.connected
      ? "Open your normal Firefox after installing the local helper."
      : state.selected
      ? "This Firefox is the selected connection."
      : state.unselected && state.profileCount === 1
        ? "This Firefox is selected automatically."
        : "Choose Use this Firefox in the profile you want to use.";
    document.querySelector("#access").textContent =
      `Connected chats: ${state.chats} · Work tabs: ${state.tabs}`;
    document.querySelector("#toggle").textContent = state.enabled
      ? "Pause"
      : "Resume";
    document.querySelector("#choose").disabled = !state.connected;
    document.querySelector("#choose").hidden = !state.connected || state.selected || (state.unselected && state.profileCount <= 1);
  } catch (e) {
    document.querySelector("#error").textContent = e.message;
    document.querySelector("#error").hidden = false;
  }
}
for (const id of ["choose", "revoke", "reset"])
  document.querySelector("#" + id).onclick = () => send(id);
document.querySelector("#toggle").onclick = () =>
  send(state?.enabled ? "pause" : "resume");
document.querySelector("#settings").onclick = async () => {
  const button = document.querySelector("#settings");
  button.disabled = true;
  document.querySelector("#error").hidden = true;
  try { await browser.runtime.sendMessage({ action: "settings" }); }
  catch (error) {
    document.querySelector("#error").textContent = error.message;
    document.querySelector("#error").hidden = false;
  } finally { button.disabled = false; }
};
send("status");
setInterval(() => send("status"), 1000);

document.querySelector("#permissions").onclick = async () => {
  await browser.permissions.request({ origins: ["http://*/*", "https://*/*"] });
  await send("status");
};
