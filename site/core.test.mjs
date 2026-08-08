import assert from "node:assert/strict";
import test from "node:test";
import { discoverWorlds, errorCode, isAllowedReturnOrigin, normalizeBrowserPath, parseBridgeFragment } from "./core.mjs";

function file(path, lastModified = 0) {
  return { name: path.split("/").at(-1), webkitRelativePath: path, lastModified };
}

test("backupを除外し複数ワールドを更新日時順に検出する", () => {
  const worlds = discoverWorlds([
    file("SaveGames/slot/old/Level.sav", 1),
    file("SaveGames/slot/old/Players/0123456789ABCDEF0123456789ABCDEF.sav"),
    file("SaveGames/slot/old/backup/2026/Level.sav", 9),
    file("SaveGames/slot/new/Level.sav", 2),
    file("SaveGames/slot/new/LevelMeta.sav"),
  ]);
  assert.deepEqual(worlds.map((world) => world.label), ["new", "old"]);
  assert.equal(worlds[1].players[0].playerUId, "0123456789abcdef0123456789abcdef");
  assert.ok(worlds[0].metadata);
});
test("path traversalと絶対パスを拒否する", () => {
  assert.equal(normalizeBrowserPath("world/../Level.sav"), null);
  assert.equal(normalizeBrowserPath("C:/world/Level.sav"), null);
  assert.equal(normalizeBrowserPath("/world/Level.sav"), null);
  assert.equal(normalizeBrowserPath("world/Level.sav"), "world/Level.sav");
});

test("bridge fragmentは完全なoriginとversion 1だけを受理する", () => {
  assert.deepEqual(
    parseBridgeFragment("#requestId=r&nonce=n&returnOrigin=https%3A%2F%2Fpaloptimizer.com&protocolVersion=1"),
    { requestId: "r", nonce: "n", returnOrigin: "https://paloptimizer.com", protocolVersion: 1 },
  );
  assert.equal(parseBridgeFragment("#requestId=r&nonce=n&returnOrigin=https%3A%2F%2Fx.test%2Fpath&protocolVersion=1"), null);
  assert.equal(parseBridgeFragment("#requestId=r&nonce=n&returnOrigin=https%3A%2F%2Fx.test&protocolVersion=2"), null);
});

test("許可originと安定error codeを判定する", () => {
  assert.equal(isAllowedReturnOrigin({ allowedReturnOrigins: ["https://paloptimizer.com"] }, "https://paloptimizer.com"), true);
  assert.equal(isAllowedReturnOrigin({}, "https://paloptimizer.com"), false);
  assert.equal(errorCode(new Error("LIMIT_EXCEEDED:too large")), "LIMIT_EXCEEDED");
  assert.equal(errorCode(new Error("secret path")), "CORRUPT_SAVE");
});
