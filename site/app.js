import { discoverWorlds, errorCode, isAllowedReturnOrigin, parseBridgeFragment, sha256Hex } from "__CORE_MODULE__";

const DECODER_VERSION = "__DECODER_VERSION__";
const SOURCE_SHA = "__SOURCE_SHA__";
const MAX_TOTAL_INPUT_BYTES = 256 * 1024 * 1024;
const MAX_PLAYER_FILES = 32;
const WORKER_TIMEOUT_MS = 120_000;
const elements = {
  picker: document.querySelector("#folder-picker"),
  choose: document.querySelector("#choose-folder"),
  worlds: document.querySelector("#worlds"),
  candidates: document.querySelector("#candidate-list"),
  status: document.querySelector("#status"),
  error: document.querySelector("#error"),
  progress: document.querySelector("#progress"),
  result: document.querySelector("#result"),
  download: document.querySelector("#download-json"),
  destination: document.querySelector("#destination"),
};

let bridge = null;
let port = null;
let resultJson = "";
let selectedSourceId = "";

async function configureBridge() {
  const candidate = parseBridgeFragment(location.hash);
  if (!candidate || !window.opener) return;
  let config = { allowedReturnOrigins: [] };
  try {
    const response = await fetch("./decoder-config.json", { cache: "no-store" });
    if (response.ok) config = await response.json();
  } catch {}
  if (!isAllowedReturnOrigin(config, candidate.returnOrigin)) {
    elements.destination.textContent = "未許可の送信先です。JSONダウンロードのみ利用できます。";
    return;
  }
  bridge = candidate;
  elements.destination.textContent = `結果の送信先: ${candidate.returnOrigin}`;
  window.addEventListener("message", (event) => {
    const message = event.data;
    if (event.origin !== candidate.returnOrigin || event.source !== window.opener) return;
    if (message?.type !== "palsav-decoder/connect" || message.requestId !== candidate.requestId || message.nonce !== candidate.nonce) return;
    if (event.ports.length !== 1) return;
    port = event.ports[0];
    port.start();
  }, { once: false });
  window.opener.postMessage({
    type: "palsav-decoder/ready",
    protocolVersion: 1,
    requestId: candidate.requestId,
    nonce: candidate.nonce,
    decoderVersion: DECODER_VERSION,
    sourceSha: SOURCE_SHA,
  }, candidate.returnOrigin);
}

elements.choose.addEventListener("click", () => elements.picker.click());
elements.picker.addEventListener("change", () => {
  elements.status.textContent = "";
  elements.candidates.replaceChildren();
  let worlds;
  try {
    worlds = discoverWorlds([...elements.picker.files]);
  } catch {
    return showError("同じワールドにLevel.savが複数あります。選び直してください。");
  }
  if (worlds.length === 0) return showError("Level.savが見つかりません。SaveGamesまたはワールドフォルダーを選択してください。");
  for (const world of worlds) {
    const item = document.createElement("li");
    const modified = world.lastModified ? new Date(world.lastModified).toLocaleString() : "不明";
    item.innerHTML = `<div><strong></strong><span>最終更新: ${modified} / Player: ${world.players.length}件</span></div>`;
    item.querySelector("strong").textContent = world.label;
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = "このワールドを読み込む";
    button.addEventListener("click", () => decodeWorld(world));
    item.append(button);
    elements.candidates.append(item);
  }
  elements.worlds.hidden = false;
  elements.worlds.querySelector("h2").focus();
});

async function decodeWorld(world) {
  if (world.players.length > MAX_PLAYER_FILES) return showError("Playerファイル数がブラウザー版の上限を超えています。");
  elements.progress.hidden = false;
  elements.result.hidden = true;
  setStage("ファイルを確認しています");
  try {
    const files = [world.level, ...(world.metadata ? [world.metadata] : []), ...world.players.map((player) => player.file)];
    const total = files.reduce((sum, file) => sum + file.size, 0);
    if (total > MAX_TOTAL_INPUT_BYTES) throw new Error("LIMIT_EXCEEDED:input total");
    // SaveGames全体を選んだ場合とworld folderだけを選んだ場合で同じIDになる。
    selectedSourceId = await sha256Hex(new TextEncoder().encode(world.label.toLowerCase()));
    const level = await world.level.arrayBuffer();
    const metadata = world.metadata ? await world.metadata.arrayBuffer() : null;
    const players = await Promise.all(world.players.map(async (player) => ({ playerUId: player.playerUId, bytes: await player.file.arrayBuffer() })));
    setStage("キャラクターデータを解析中");
    resultJson = await runWorker({ level, metadata, players });
    setStage("PalOptimizerへ結果を返します");
    await deliverResult(resultJson, selectedSourceId);
    elements.progress.hidden = true;
    elements.result.hidden = false;
    elements.result.querySelector("h2").focus();
  } catch (error) {
    elements.progress.hidden = true;
    showError(userMessage(errorCode(error)));
  }
}

function runWorker(payload) {
  return new Promise((resolve, reject) => {
    const worker = new Worker("__DECODER_WORKER__", { type: "module" });
    const transfer = [payload.level, ...payload.players.map((player) => player.bytes)];
    if (payload.metadata) transfer.push(payload.metadata);
    const timeout = setTimeout(() => {
      worker.terminate();
      reject(new Error("WORKER_TIMEOUT"));
    }, WORKER_TIMEOUT_MS);
    worker.addEventListener("message", (event) => {
      clearTimeout(timeout);
      worker.terminate();
      if (event.data?.type === "result") resolve(event.data.json);
      else reject(new Error(event.data?.message || "WORKER_TRAPPED"));
    }, { once: true });
    worker.addEventListener("error", () => {
      clearTimeout(timeout);
      worker.terminate();
      reject(new Error("WORKER_TRAPPED"));
    }, { once: true });
    worker.postMessage({ type: "decode", ...payload }, transfer);
  });
}

async function deliverResult(json, sourceWorldId) {
  if (!bridge || !port) return;
  const payload = new TextEncoder().encode(json).buffer;
  const payloadSha256 = await sha256Hex(payload);
  const document = JSON.parse(json);
  const envelope = {
    type: "palsav-decoder/result",
    protocolVersion: 1,
    requestId: bridge.requestId,
    decoderVersion: DECODER_VERSION,
    documentSchemaVersion: 1,
    sourceSha: SOURCE_SHA,
    sourceWorldId,
    payloadEncoding: "utf-8-json",
    payloadByteLength: payload.byteLength,
    payloadSha256,
    warnings: Array.isArray(document.warnings) ? document.warnings : [],
  };
  port.postMessage({ type: "palsav-decoder/result-header", envelope });
  port.postMessage(payload, [payload]);
}

elements.download.addEventListener("click", () => {
  if (!resultJson) return;
  const url = URL.createObjectURL(new Blob([resultJson], { type: "application/json" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = "world-document.v1.json";
  link.click();
  URL.revokeObjectURL(url);
});

function setStage(value) {
  elements.status.textContent = value;
}

function showError(message) {
  elements.error.textContent = message;
  elements.error.focus();
}

function userMessage(code) {
  return ({
    MISSING_LEVEL: "Level.savが見つかりません。",
    UNSUPPORTED_FORMAT: "このセーブ形式は現在のDecoderでは対応していません。",
    LIMIT_EXCEEDED: "セーブデータがブラウザー版の安全上限を超えています。セルフホスト版またはSync Agentをご利用ください。",
    WORKER_TIMEOUT: "解析が制限時間を超えました。タブを再読み込みしてお試しください。",
    WORKER_TRAPPED: "解析Workerを安全に停止しました。タブを再読み込みしてお試しください。",
    CORRUPT_SAVE: "セーブデータを解析できませんでした。破損または未対応形式の可能性があります。",
  })[code] || "セーブデータを解析できませんでした。";
}

configureBridge();
