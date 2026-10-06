# Fix-issue batch 5292–5299 (fetched 2026-10-05)

| # | Sev | Crate(s) | Summary |
|---|-----|----------|---------|
| 5292 | LOW | byroredux bin + debug-server + core | `debug_drain_system` name is a hand-typed literal in app_events.rs; `run_exclusive_named` result discarded. Export shared const, `debug_assert!` on result. |
| 5293 | LOW | byroredux bin + scripting | Bulk Talk filter rebuilds whole alias table + 4 entity sets every frame in `populate_candidates`. Cache keyed on generations. |
| 5294 | LOW | tools/byro-launcher + game-detect | `--profiles` parses loosely (typo → silently retargets real writes); engine never reads custom path. Shared `user_profiles_path()`, strict parse, document/carry path. |
| 5295 | LOW | crates/plugin (esm) | INFO `DATA` decode wrong: u16 spans Next Speaker + Flags 1; Goodbye is 0x0100 not 0x80; Flags 2 dropped; Skyrim authors 8-B DATA. Split decode. |
| 5296 | LOW | crates/plugin (esm) | TRNS "Around Origin" tests `flags & 0x8000` but vanilla uses `0x10000` (xEdit flag *index* 16). Fix bit + test fixture. |
| 5297 | HIGH | scripting + byroredux bin | Spoken-line path polls fragment journal in Stage::Late, claims cursor, re-emits batch that `event_cleanup_system` drains unread → stage fragment never runs. Return direct advances without polling. |
| 5298 | MED | scripting | Player-decline rule in SetUnconscious/StartCombat/StopCombat is syntactic; `PlayerRef` property resolves to player at runtime since #4694. Decline at apply time when resolved actor is player. |
| 5299 | LOW | crates/plugin (esm) | Starfield XRGD ragdoll-pose positions are metric; `spatial_units::cell()` never lifts them. Add `vector()` lift per bone. |

Order (grouped by crate): 5296 → 5299 → 5295 (plugin) → 5298 → 5297 (scripting) → 5292 (debug-server/bin) → 5293 (bin perf) → 5294 (launcher/game-detect).

Toolchain note: rust-toolchain.toml pins 1.96.0; distro cargo shadows rustup shims — use
`TC=$(rustup which --toolchain 1.96.0 cargo)` and invoke `"$TC"` directly. Bin-crate tests:
`PATH="$(dirname "$TC"):$PATH" "$TC" test -p byroredux --bin byroredux`.
