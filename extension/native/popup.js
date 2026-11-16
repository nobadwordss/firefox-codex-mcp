"use strict";
let state;
async function send(action) {
  try {
    state = await browser.runtime.sendMessage({ action });
    document.querySelector("#permissions").hidden = state.siteAccess;
    document.querySelector("#status").textContent = state.enabled
      ? state.status
      : "잠시 중지됨. 모든 작업 탭 접근이 해제됐습니다.";
    document.querySelector("#profile").textContent = state.selected
      ? "이 Firefox가 기본 연결입니다."
      : state.unselected && state.profileCount === 1
        ? "이 Firefox가 자동으로 사용됩니다."
        : "사용할 Firefox에서 ‘이 Firefox 사용’을 누르세요.";
    document.querySelector("#access").textContent =
      `연결된 채팅 ${state.chats} · 작업 탭 ${state.tabs}`;
    document.querySelector("#toggle").textContent = state.enabled
      ? "잠시 중지"
      : "다시 연결";
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
