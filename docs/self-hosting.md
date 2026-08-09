# Web Decoder self-hosting

Use the versioned `palsav-web-vX.Y.Z.zip` release asset. Verify the adjacent SHA-256 file before
extracting it, then serve the extracted directory without rewriting asset names or contents.

Required hosting behavior:

- HTTPS only.
- `application/wasm` for `.wasm`.
- `X-Content-Type-Options: nosniff` and `Referrer-Policy: no-referrer`.
- CSP equivalent to `default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; connect-src 'self'; style-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'`.
- `index.html` and `decoder-config.json`: revalidate or short cache.
- content-hashed files below `assets/`: immutable long cache.
- no upload endpoint, request-body logging, analytics injection, advertisement, external font, or CDN script.

Copy `decoder-config.example.json` to `decoder-config.json`, replace the examples with exact allowed
PalOptimizer origins, and do not use wildcard or suffix matching. The PalOptimizer deployment must
pin this decoder origin and version. Do not silently fail over to another origin.

The host only receives requests for static HTML, JavaScript, WebAssembly, configuration, and notices.
Raw `.sav` bytes are read by browser file APIs and remain inside the browser. Normal host access logs
may still record IP addresses and user agents; disclose that in the host's privacy notice.

GitHub Pages is a reference deployment with no SLA. It is suitable for evaluating the UI at small
scale, not as PalOptimizer's production dependency. High-traffic operators should use their own
static hosting and bandwidth policy.
