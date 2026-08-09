# ADR: untrusted saves and cross-origin delivery

Status: accepted for Web Decoder v0.2.0.

Raw saves are untrusted, potentially very large binary inputs. Decoding therefore runs in a dedicated
single-thread Worker with no network code. Each import creates a new Worker; success, error, trap, or
120-second timeout terminates it and discards the WebAssembly linear memory. The browser surface caps
Level input at 192 MiB, metadata at 8 MiB, each player at 32 MiB, player count at 32, aggregate input
at 256 MiB, declared decompressed output at 384 MiB, and result JSON at 96 MiB. These are independent
from the native decoder's 512 MiB defensive guard and prevent the browser from adopting that value as
an unchecked default. The limits provide headroom over the repository's tens-of-megabytes fixtures;
production acceptance must record representative real-save measurements before raising them.

Cross-origin risks include a malicious opener, popup replacement, oversized message, stale protocol,
and payload substitution. Exact origin plus window source checks, a cryptographic nonce/request pair,
a transferred MessagePort, version pinning, byte bounds, and SHA-256 address those risks. A decoder
not configured for the requesting origin remains a standalone JSON downloader.

Storage quota and browser eviction are PalOptimizer concerns. The decoder never persists save bytes or
WorldDocument. PalOptimizer persists only its validated, converted snapshot and preserves the previous
transaction on any failure.
