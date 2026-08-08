import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const dist = join(root, "dist", "web");
const generated = join(root, "target", "wasm-bindgen-web");
const assets = join(dist, "assets");
const sourceSha = process.env.GITHUB_SHA || execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim();
const cargo = readFileSync(join(root, "Cargo.toml"), "utf8");
const version = cargo.match(/\[workspace\.package\][\s\S]*?version\s*=\s*"([^"]+)"/)?.[1];
if (!version) throw new Error("Workspace version was not found.");

rmSync(dist, { recursive: true, force: true });
rmSync(generated, { recursive: true, force: true });
mkdirSync(assets, { recursive: true });
mkdirSync(generated, { recursive: true });

execFileSync("cargo", ["build", "--release", "--locked", "--target", "wasm32-unknown-unknown", "--package", "palsav-decoder-wasm"], { cwd: root, stdio: "inherit" });
execFileSync("wasm-bindgen", [
  join(root, "target", "wasm32-unknown-unknown", "release", "palsav_decoder_wasm.wasm"),
  "--target", "web",
  "--out-dir", generated,
  "--out-name", "palsav_decoder",
], { cwd: root, stdio: "inherit" });

function sha(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function emit(prefix, extension, bytes) {
  const hash = sha(bytes).slice(0, 16);
  const name = `${prefix}.${hash}.${extension}`;
  writeFileSync(join(assets, name), bytes);
  return `./assets/${name}`;
}

const wasmBytes = readFileSync(join(generated, "palsav_decoder_bg.wasm"));
const wasmAsset = emit("palsav_decoder", "wasm", wasmBytes);
let bindingSource = readFileSync(join(generated, "palsav_decoder.js"), "utf8")
  .replace("palsav_decoder_bg.wasm", basename(wasmAsset));
const bindingAsset = emit("palsav_decoder", "js", bindingSource);
const coreAsset = emit("core", "mjs", readFileSync(join(root, "site", "core.mjs")));
const workerSource = readFileSync(join(root, "site", "decoder.worker.js"), "utf8")
  .replace("__WASM_BINDGEN_JS__", bindingAsset.replace("./assets/", "./"));
const workerAsset = emit("decoder.worker", "js", workerSource);
const appSource = readFileSync(join(root, "site", "app.js"), "utf8")
  .replace("__CORE_MODULE__", coreAsset.replace("./assets/", "./"))
  .replace("__DECODER_WORKER__", workerAsset)
  .replaceAll("__DECODER_VERSION__", version)
  .replaceAll("__SOURCE_SHA__", sourceSha);
const appAsset = emit("app", "js", appSource);
const styleAsset = emit("style", "css", readFileSync(join(root, "site", "style.css")));
const html = readFileSync(join(root, "site", "index.html"), "utf8")
  .replace("__APP_ASSET__", appAsset)
  .replace("__STYLE_ASSET__", styleAsset);
writeFileSync(join(dist, "index.html"), html);

cpSync(join(root, "site", "decoder-config.example.json"), join(dist, "decoder-config.example.json"));
for (const name of ["LICENSE", "COPYRIGHT", "README.md", "THIRD_PARTY_NOTICES.md"]) cpSync(join(root, name), join(dist, name));
writeFileSync(join(dist, "SOURCE.txt"), `Corresponding source: https://github.com/nuitsjp/palsav-decoder/tree/${sourceSha}\n`);

const artifactFiles = [
  "index.html",
  "decoder-config.example.json",
  "LICENSE",
  "COPYRIGHT",
  "README.md",
  "THIRD_PARTY_NOTICES.md",
  "SOURCE.txt",
  ...[appAsset, styleAsset, coreAsset, workerAsset, bindingAsset, wasmAsset].map((value) => value.replace("./", "")),
];
const checksums = Object.fromEntries(artifactFiles.map((name) => [name.replaceAll("\\", "/"), sha(readFileSync(join(dist, name)))]));
writeFileSync(join(dist, "decoder-manifest.json"), `${JSON.stringify({
  decoderVersion: version,
  bridgeProtocolVersion: 1,
  documentSchemaVersion: 1,
  sourceCommitSha: sourceSha,
  artifacts: checksums,
}, null, 2)}\n`);

console.log(`Built Web Decoder ${version} (${sourceSha}) at ${dist}`);
