import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

const workflow = readFileSync(new URL("../.github/workflows/pages.yml", import.meta.url), "utf8");
const block = workflow.match(/- name: Download and verify exact published release artifact[\s\S]*?        run: \|\r?\n((?:          .*\r?\n)+)/);
assert.ok(block, "Pagesの検証stepが見つかりません。");
const verification = block[1].split(/\r?\n/).map((line) => line.slice(10)).join("\n");
const manifestSha = "a".repeat(40);
const bash = process.platform === "win32" ? join(process.env.ProgramFiles, "Git", "bin", "bash.exe") : "bash";

const ghFixture = `
import { createHash } from "node:crypto";
import { copyFileSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
const fixture = JSON.parse(readFileSync(process.env.PAGES_TEST_CONFIG, "utf8"));
const args = process.argv.slice(2);
if (args[0] === "api") {
  let response;
  if (args[1].includes("/releases/tags/")) {
    response = { draft: false, prerelease: false };
  } else if (args[1].includes("/git/ref/tags/")) {
    response = { object: { type: fixture.tagType, sha: fixture.tagType === "tag" ? "b".repeat(40) : fixture.commitSha } };
  } else if (args[1].includes("/commits/refs/tags/")) {
    response = { sha: fixture.commitSha };
  } else {
    throw new Error("想定外のGitHub APIです。");
  }
  const query = args[args.indexOf("--jq") + 1];
  const value = query === ".object.sha" ? response.object.sha : query === ".sha" ? response.sha : JSON.stringify(response);
  process.stdout.write(value + "\\n");
} else if (args[0] === "release" && args[1] === "download") {
  const directory = args[args.indexOf("--dir") + 1];
  const name = "palsav-web-v0.2.3.zip";
  copyFileSync(fixture.archive, join(directory, name));
  const digest = createHash("sha256").update(readFileSync(fixture.archive)).digest("hex");
  writeFileSync(join(directory, name + ".sha256"), digest + "  " + name + "\\n");
} else {
  throw new Error("想定外のGitHub CLI操作です。");
}
`;

function verifyRelease(t, { tagType, commitSha = manifestSha }) {
  const root = mkdtempSync(join(tmpdir(), "palsav-pages-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const repository = join(root, "archive-source");
  mkdirSync(repository);
  writeFileSync(join(repository, "decoder-manifest.json"), JSON.stringify({ decoderVersion: "0.2.3", sourceCommitSha: manifestSha }));
  execFileSync("git", ["init", "--quiet", repository]);
  execFileSync("git", ["-C", repository, "add", "decoder-manifest.json"]);
  const tree = execFileSync("git", ["-C", repository, "write-tree"], { encoding: "utf8" }).trim();
  const archive = join(root, "fixture.zip");
  writeFileSync(archive, execFileSync("git", ["-C", repository, "archive", "--format=zip", tree]));
  const stub = join(root, "gh-fixture.mjs");
  const config = join(root, "fixture.json");
  writeFileSync(stub, ghFixture);
  writeFileSync(config, JSON.stringify({ archive, tagType, commitSha }));
  const result = spawnSync(bash, ["--noprofile", "--norc", "-c", `gh() { "$PAGES_TEST_NODE" "$PAGES_TEST_GH" "$@"; }\n${verification}`], {
    cwd: root,
    encoding: "utf8",
    timeout: 30_000,
    env: {
      ...process.env,
      GITHUB_REPOSITORY: "fixture/decoder",
      RELEASE_TAG: "v0.2.3",
      PAGES_TEST_NODE: process.execPath.replaceAll("\\", "/"),
      PAGES_TEST_GH: stub.replaceAll("\\", "/"),
      PAGES_TEST_CONFIG: config,
    },
  });
  assert.ifError(result.error);
  assert.match(result.stdout, /palsav-web-v0\.2\.3\.zip: OK/, result.stderr);
  return result;
}

test("注釈付きtagが指すcommitとmanifestが一致すればPagesへ受理する", (t) => {
  const result = verifyRelease(t, { tagType: "tag" });
  assert.equal(result.status, 0, result.stdout + result.stderr);
});

test("軽量tagが指すcommitとmanifestが一致すればPagesへ受理する", (t) => {
  const result = verifyRelease(t, { tagType: "commit" });
  assert.equal(result.status, 0, result.stdout + result.stderr);
});

test("checksumが正しくてもtagのcommitと異なるmanifestを拒否する", (t) => {
  const result = verifyRelease(t, { tagType: "tag", commitSha: "c".repeat(40) });
  assert.notEqual(result.status, 0);
});
