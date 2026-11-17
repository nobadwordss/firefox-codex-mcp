"use strict";
let state;
async function send(action) {
  try {
    state = await browser.runtime.sendMessage({ action });
    document.querySelector("#permissions").hidden = state.siteAccess;
    document.querySelector("#status").textContent = state.enabled
      ? state.status
      : "Paused. All work-tab access has been revoked.";
    document.querySelector("#profile").textContent = state.selected
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
  } catch (e) {
    document.querySelector("#status").textContent = e.message;
  }
}
for (const id of ["choose", "revoke", "reset"])
  document.querySelector("#" + id).onclick = () => send(id);
document.querySelector("#toggle").onclick = () =>
  send(state?.enabled ? "pause" : "resume");
send("status");
setInterval(() => send("status"), 1000);

document.querySelector("#permissions").onclick = async () => {
  await browser.permissions.request({ origins: ["http://*/*", "https://*/*"] });
  await send("status");
};
