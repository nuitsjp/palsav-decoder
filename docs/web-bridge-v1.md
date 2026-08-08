# Web bridge v1

PalOptimizer opens a version-pinned decoder URL in a new tab. The URL fragment carries only
`requestId`, a cryptographic `nonce`, exact `returnOrigin`, and `protocolVersion=1`. Local paths,
filenames, world IDs, instance IDs, and save contents never enter the URL.

After loading `decoder-config.json`, the decoder sends `palsav-decoder/ready` only when the return
origin is an exact allow-list member. PalOptimizer verifies `event.origin`, `event.source`, request,
nonce, protocol, decoder version, and source SHA. It transfers one `MessageChannel` port with an exact
`targetOrigin`; all result traffic then uses that port.

The decoder sends `palsav-decoder/result-header`, followed by a transferable UTF-8 JSON
`ArrayBuffer`. The header envelope has exactly these keys:

```text
type, protocolVersion, requestId, decoderVersion, documentSchemaVersion,
sourceSha, sourceWorldId, payloadEncoding, payloadByteLength, payloadSha256, warnings
```

`sourceWorldId` is a SHA-256 identifier derived locally from the selected world root and is used only
to replace the same local record. PalOptimizer validates exact keys, all versions and identifiers,
the byte limit, actual byte length, SHA-256, and the full WorldDocument schema before one IndexedDB
transaction. Unknown or prototype-related keys are rejected.

Stable errors are `MISSING_LEVEL`, `UNSUPPORTED_FORMAT`, `CORRUPT_SAVE`, `LIMIT_EXCEEDED`,
`WORKER_TRAPPED`, `WORKER_TIMEOUT`, `PROTOCOL_MISMATCH`, and `STORAGE_QUOTA`. Optional player
failures produce `playerDataPartiallyUnavailable` without identifying a player or path.
