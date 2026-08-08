const PLAYER_PATTERN = /^Players\/([0-9a-f]{32})\.sav$/i;

export function normalizeBrowserPath(value) {
  if (typeof value !== "string" || value.includes("\\") || value.startsWith("/") || /^[A-Za-z]:/.test(value)) return null;
  const parts = value.split("/").filter(Boolean);
  if (parts.length === 0 || parts.some((part) => part === "." || part === "..")) return null;
  return parts.join("/");
}

export function discoverWorlds(files) {
  const normalized = [];
  for (const file of files) {
    const path = normalizeBrowserPath(file.webkitRelativePath || file.name);
    if (!path || path.split("/").some((part) => part.toLowerCase() === "backup")) continue;
    normalized.push({ file, path });
  }

  const levels = normalized.filter(({ path }) => path.endsWith("/Level.sav") || path === "Level.sav");
  const worlds = [];
  const roots = new Set();
  for (const level of levels) {
    const root = level.path === "Level.sav" ? "" : level.path.slice(0, -"/Level.sav".length);
    if (roots.has(root)) throw new Error("DUPLICATE_LEVEL");
    roots.add(root);
    const underRoot = normalized
      .filter(({ path }) => root === "" || path.startsWith(`${root}/`))
      .map(({ file, path }) => ({ file, relative: root === "" ? path : path.slice(root.length + 1) }));
    const metadata = underRoot.find(({ relative }) => relative === "LevelMeta.sav")?.file ?? null;
    const players = underRoot
      .map(({ file, relative }) => ({ file, match: relative.match(PLAYER_PATTERN) }))
      .filter(({ match }) => Boolean(match))
      .map(({ file, match }) => ({ file, playerUId: match[1].toLowerCase() }))
      .sort((left, right) => left.playerUId.localeCompare(right.playerUId, "en"));
    worlds.push({
      root,
      label: root.split("/").filter(Boolean).at(-1) || "選択したワールド",
      level: level.file,
      metadata,
      players,
      lastModified: level.file.lastModified || 0,
    });
  }
  return worlds.sort((left, right) => right.lastModified - left.lastModified || left.root.localeCompare(right.root, "en"));
}

export function parseBridgeFragment(fragment) {
  const params = new URLSearchParams(String(fragment || "").replace(/^#/, ""));
  const requestId = params.get("requestId");
  const nonce = params.get("nonce");
  const returnOrigin = params.get("returnOrigin");
  const protocolVersion = Number(params.get("protocolVersion"));
  if (!requestId || !nonce || protocolVersion !== 1) return null;
  try {
    const origin = new URL(returnOrigin).origin;
    if (origin !== returnOrigin) return null;
    return { requestId, nonce, returnOrigin: origin, protocolVersion };
  } catch {
    return null;
  }
}

export function isAllowedReturnOrigin(config, origin) {
  return Array.isArray(config?.allowedReturnOrigins)
    && config.allowedReturnOrigins.some((allowed) => allowed === origin);
}

export function errorCode(error) {
  const message = error instanceof Error ? error.message : String(error);
  const match = message.match(/(?:^|:)\b(MISSING_LEVEL|UNSUPPORTED_FORMAT|CORRUPT_SAVE|LIMIT_EXCEEDED|WORKER_TRAPPED|WORKER_TIMEOUT)\b/);
  return match?.[1] ?? "CORRUPT_SAVE";
}

export async function sha256Hex(bytes) {
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((value) => value.toString(16).padStart(2, "0")).join("");
}

