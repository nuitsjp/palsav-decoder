import init, { WebDecoder } from "__WASM_BINDGEN_JS__";

let initialized;

self.addEventListener("message", async (event) => {
  if (event.data?.type !== "decode") return;
  try {
    initialized ??= init();
    await initialized;
    const decoder = new WebDecoder(new Uint8Array(event.data.level));
    if (event.data.metadata) decoder.set_metadata(new Uint8Array(event.data.metadata));
    for (const player of event.data.players) {
      decoder.add_player(player.playerUId, new Uint8Array(player.bytes));
    }
    const json = decoder.finish_json();
    self.postMessage({ type: "result", json });
  } catch (error) {
    self.postMessage({ type: "error", message: String(error) });
  }
});

