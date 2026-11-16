(() => {
  "use strict";
  if (globalThis.__firefoxCodexMcpLoaded) return;
  globalThis.__firefoxCodexMcpLoaded = true;
  const protocol = FirefoxCodexProtocol, page = FirefoxCodexPage();
  let channel = null, pageActive = true;
  function close(binding) {
    if (channel === binding) { channel = null; page.revoke(); }
    binding.port.disconnect();
  }
  addEventListener("pagehide", () => { pageActive = false; if (channel) close(channel); });
  addEventListener("pageshow", () => { pageActive = true; });
  browser.runtime.onConnect.addListener(port => {
    if (port.sender?.id !== browser.runtime.id || port.name !== protocol.PORT_NAME || !pageActive) { port.disconnect(); return; }
    if (channel) close(channel);
    page.revoke();
    const binding = { port, context: null };
    channel = binding;
    port.onDisconnect.addListener(() => {
      if (channel === binding) { channel = null; page.revoke(); }
    });
    port.onMessage.addListener(request => {
      if (channel !== binding || !pageActive) return;
      try {
        if (request?.kind === "grant" && binding.context === null && protocol.validContext(request.context) && request.context.documentId === page.documentId) {
          binding.context = { ...request.context };
          page.bind(binding.context);
          port.postMessage({ kind: "ready", context: binding.context });
          return;
        }
        if (request?.kind !== "command" || !protocol.sameContext(request.context, binding.context)) { close(binding); return; }
        // From this check through the action there is no asynchronous yield.
        const response = page.execute(request);
        port.postMessage({ kind: "response", id: request.id, context: binding.context, ...response });
      } catch { close(binding); }
    });
    port.postMessage({ kind: "hello", documentId: page.documentId });
  });
})();
