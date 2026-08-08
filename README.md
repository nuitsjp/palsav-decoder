# palsav-decoder

`palsav` is a standalone CLI that decodes Palworld save files into application-neutral JSON or NDJSON.
The repository is also prepared for a Web API delivery surface that will use the same shared decoder
without coupling HTTP concerns to the CLI.

## License

Copyright (C) 2026 Atsushi Nakamura.

This entire repository is licensed under the [GNU General Public License v3.0 or later](LICENSE).
The CLI runs as an independent process. Consuming applications pass an input path as a command-line
argument, read application-neutral JSON or NDJSON from stdout, and receive diagnostics on stderr.
The CLI does not provide a stable in-process library ABI. The workspace-internal shared crate exists
for the GPL-licensed CLI and Web API implementations in this repository.

Binary releases include the GPL license, source location, third-party notices, and generated Rust
dependency license texts. See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

Palworld and all related names and data belong to their respective rights holders. This repository does not include the game, game assets, DLLs, or actual save data.

## Repository layout

```text
cli/
  src/lib.rs                 CLI facade
  src/implementation/        argument parsing and command execution details
  src/main.rs                thin executable entry point
web-api/
  src/lib.rs                 Web API application facade
  src/implementation/        Web-specific application details
shared/
  src/lib.rs                 shared decoding and document-contract facade
  src/implementation/        decoder, schema, model, and save-format details
```

Each delivery surface depends on the shared facade. The CLI never depends on Web API code, and HTTP
transport, upload handling, authentication, and background execution will remain below the Web API
facade when they are added.

## Usage

```text
palsav decode world --input <world-directory-or-Level.sav> [--format json|ndjson]
palsav decode players --input <world-directory-or-Players-directory> [--format json|ndjson]
palsav decode player --input <Players/PlayerUId.sav> [--format json|ndjson]
palsav decode meta --input <LevelMeta.sav> [--format json|ndjson]
palsav --version
```

JSON is the default format. On success, stdout contains data only and diagnostics are written to stderr. Failures return a non-zero exit code. Output never includes the local input path.

The input path may contain spaces or Unicode characters. The CLI reads save files locally and never
performs network access.

### JSON

`decode world` returns the following top-level contract:

```json
{
  "schemaVersion": 1,
  "worldName": "Example",
  "characters": [],
  "playerContainers": {
    "palStorageContainerIds": [],
    "otomoContainerIds": []
  },
  "baseCamps": [],
  "world": null,
  "playerRelics": [],
  "warnings": []
}
```

The command fails if the required character extraction fails. If optional data cannot be read, the
command returns all available data and adds stable, path-free warning codes to `warnings`:

- `baseCampsUnavailable`: base-camp extraction failed.
- `levelMetaUnavailable`: LevelMeta was absent, unreadable, or invalid.
- `playerDataPartiallyUnavailable`: the Players directory or at least one player entry was unreadable or invalid.
- `worldOverviewUnavailable`: a world section failed or the result violated contract bounds.

`baseCamps` and `world` are nullable. Other arrays are always present and may be empty.

`decode player` returns `schemaVersion`, optional Pal storage and Otomo container IDs, an optional
world point, and relic state. `decode meta` returns `schemaVersion` and a nullable `worldName`.
`decode players` returns `schemaVersion`, all valid `playerRelics`, and `warnings` after scanning
the Players directory once. It is intended for polling clients that must avoid one process per player.

### NDJSON

For `decode world`, `--format ndjson` emits one typed record per line: `metadata`, `character`,
`playerContainers`, `baseCamp`, `world`, `playerRelics`, `warning`, and `end`. The `end` record
contains `characterCount` and `playerRelicCount`.

For `decode players`, NDJSON emits `playerRelics`, `warning`, and `end` records. For `decode player`,
it emits one `player` record. For `decode meta`, it emits one `metadata` record. The latter two
single-document commands do not emit an `end` record.

Use NDJSON to process large world saves without retaining the entire decoded document in memory.

### Exit codes

- `0`: decoding and serialization succeeded, including successful partial output with warnings.
- `1`: the input could not be read, decompressed, decoded, validated, or serialized.
- `2`: command-line usage was invalid.

## Verifying a release

Download the versioned Windows ZIP and its `.sha256` file from the same GitHub Release, then run:

```powershell
$expected = (Get-Content .\palsav-v0.1.0-windows-x86_64.zip.sha256).Split()[0]
$actual = (Get-FileHash .\palsav-v0.1.0-windows-x86_64.zip -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw "Checksum mismatch" }
Expand-Archive .\palsav-v0.1.0-windows-x86_64.zip -DestinationPath .\palsav-v0.1.0
```

The corresponding source is the Git tag with the same version at
<https://github.com/nuitsjp/palsav-decoder>.

Windows releases are not Authenticode-signed. GitHub Actions publishes a build-provenance
attestation for each archive in addition to the SHA-256 checksum. Verify it with GitHub CLI:

```powershell
gh attestation verify .\palsav-v0.1.0-windows-x86_64.zip --repo nuitsjp/palsav-decoder
```

## Versioning

`schemaVersion` versions the data contract independently of the CLI. Backward-compatible field additions retain the current version. Changes that break the meaning, type, or required status of an existing field increment it.

## Development

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo llvm-cov --workspace --all-targets --locked --fail-under-lines 85 --fail-under-functions 80 --fail-under-regions 80
cargo build --release --locked --package palsav-decoder-cli
```

The release workflow validates that a version tag points to `main`, builds a license-complete Windows
ZIP, and creates a Draft GitHub Release. A consuming application can launch the independently
downloaded executable instead of bundling the CLI in its own distribution.
