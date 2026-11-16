"use strict";
const statusElement = document.querySelector("#status");
async function update() {
  const state = await browser.runtime.sendMessage({ kind: "status" });
  statusElement.textContent = `${state.connectionState}. ${state.tabId === null ? "No tab enabled." : `Enabled tab: ${state.tabId}.`}`;
  document.querySelector("#enable").disabled = !state.ready;
}
for (const kind of ["connect", "enable", "disconnect"]) {
  document.querySelector(`#${kind}`).addEventListener("click", async () => {
    try {
      await browser.runtime.sendMessage({ kind, ...(kind === "connect" ? { token: document.querySelector("#token").value } : {}) });
      if (kind === "connect") document.querySelector("#token").value = "";
      await update();
    } catch (error) { statusElement.textContent = error.message; }
  });
}
update().catch(error => { statusElement.textContent = error.message; });
// Poll only while this small popup is open, to show connection progress.
const timer = setInterval(() => update().catch(() => {}), 1500);
addEventListener("unload", () => clearInterval(timer));
