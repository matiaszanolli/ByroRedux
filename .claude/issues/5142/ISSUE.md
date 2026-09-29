# #5142: TOOL-D1-2026-09-29-03: `63c0aee3b`'s release debug-server opt-in and bare-filename screenshot rule broke every byro-dbg harness that predates it — capture.sh, m-exteriors, m34, m-trees, m43, m47, m48-4..7 HUD, r6a, material-provider-matrix, plus 4 docs

**Labels**: medium, bug, tech-debt, test-gap, doc-rot

**Source report**: `docs/audits/AUDIT_TOOLING_2026-09-29.md`
**Severity**: MEDIUM
**Dimension**: Debug Trust Boundary (harness fallout). Consolidates RT-1 (AUDIT_RUNTIME), EXT-D7-2026-09-29-01 (AUDIT_EXTERIOR) and SPT-D3-01 (AUDIT_SPEEDTREE) — one root cause.

## Location
Root cause (`63c0aee3b`, 2026-09-27, the #4752 fix):
- `byroredux/src/main.rs` `debug_server_allowed` — release builds need `BYRO_DEBUG_SERVER=1` (`debug_build || opt_in ∈ {1,true,yes}`)
- `crates/debug-server/src/system.rs` `write_screenshot` — accepts only a single bare filename under `./screenshots/` (test `rejects_absolute_and_parent_paths`)

**A. Release engine launched without `BYRO_DEBUG_SERVER=1` (byro-dbg cannot attach):**
- `.claude/commands/audit-runtime/capture.sh` — launch env sets only `BYROREDUX_SETTINGS_PATH`; every capture ends in `FATAL debug server never came up` (RT-1)
- `docs/smoke-tests/m34-day-night.sh` (EXT-D7-01)
- `docs/smoke-tests/m-trees.sh` (SPT-D3-01)
- `docs/smoke-tests/m43-quest-runtime.sh`
- `docs/smoke-tests/m47-triggers.sh`
- `docs/smoke-tests/r6a_stale_15_bench.sh`
- `scripts/material-provider-matrix.sh`

**B. Directory/absolute screenshot paths the server now rejects** ("screenshot path must be a filename inside screenshots/"):
- `docs/smoke-tests/m-exteriors.sh` — 12 `screenshot $profile_dir/<name>.png` commands in the cycle/water profiles (EXT-D7-01; this script already sets the opt-in)
- `docs/smoke-tests/m48-4-oblivion-hud.sh` (`screenshot $OUT_DIR/full.png`, `pinned.png`)
- `docs/smoke-tests/m48-5-fnv-hud.sh`
- `docs/smoke-tests/m48-5-fo3-hud.sh`
- `docs/smoke-tests/m48-6-skyrim-hud.sh`
- `docs/smoke-tests/m48-7-fo4-hud.sh`
- `scripts/material-provider-matrix.sh` (`printf 'screenshot %s\n' "${run_out}/${mode}.png"`; the later "screenshot missing" check hard-fails)

**C. Docs that teach "release `--bench-hold` → attach byro-dbg" with no opt-in / no filename rule:**
- `CLAUDE.md` (Usage: `cargo run --release -- … --bench-hold` "HOLD open for byro-dbg"; gotcha bullet)
- `README.md` (`cargo run --release -- --studio --bench-hold`, "Scriptable over byro-dbg")
- `docs/engine/testing.md` (`--bench-hold` → `byro-dbg`-attach pattern)
- `docs/smoke-tests/README.md` (the `--bench-hold` → byro-dbg pattern; mentions neither rule — only `docs/engine/debug-cli.md` does)
- `.claude/commands/audit-runtime` SKILL.md invocation paragraph (RT-1)

## Description
`63c0aee3b` made two changes: `write_screenshot` now accepts only a bare filename, and `debug_server_allowed(false, None)` is now false in release builds. Harnesses fixed at the time use one of two patterns — bare-name capture then `mv "$SMOKE_DATA/screenshots/$name"` (`p3-hud.sh`, `p3-player-body.sh`), or `env … BYRO_DEBUG_SERVER=1` (p0–p5, w1, m41, m48-menu-load). The harnesses above got neither.

Failure modes:
- **capture.sh (RT-1):** the engine logs `Debug server disabled (set BYRO_DEBUG_SERVER=1 to opt in)` then `bench-hold-unavailable: … the debug server did not bind`; the script's 90 s ping loop then dies with a generic `FATAL debug server never came up`. `/audit-runtime` cannot produce a result as documented.
- **m-exteriors cycle/water (EXT-D7-01):** each capture is rejected, the heredoc's `|| true` swallows it, and `image_health` HARD FAILs on every phase frame (cycle) and on surface/underwater frames (water). The `composite_term` captures (#4491) — skyal.md §4's only pixel evidence for sky assembly — are never written.
- **m34-day-night (EXT-D7-01):** byro-dbg cannot attach; all seven `require_output` checks FAIL.
- **m-trees (SPT-D3-01):** byro-dbg cannot attach, `|| true` swallows it, the `(N entities)` grep falls back to `echo 0`, FNV and FO3 HARD FAIL on `Billboard entities=0 < floor`, and the script blames the (healthy) `.spt` route: "zero indicates the .spt extension switch isn't routing". The harness also has no Oblivion arm (`[fnv|fo3|all]`), though Oblivion holds 113 of 133 vanilla `.spt` files.
- **m48-4..7 HUD smokes:** they do export `BYRO_DEBUG_SERVER=1` but pass `$OUT_DIR/*.png` from `mktemp -d` (absolute `/tmp` path), so every capture is refused and the pixel gates find no PNG. `m48-5-fnv-hud.sh` was edited in `db8351587` and is still broken.
- **m43, m47, r6a, material-provider-matrix:** release launches with no opt-in. `r6a_stale_15_bench.sh` is also independently broken: it runs `cd "$game_data" && cargo run --release` (outside the workspace, no `Cargo.toml`) and passes `byro-dbg -p "$PORT"`, while byro-dbg ignores every argument except `--tui`.

## Evidence
- `main.rs`: `debug_build || matches!(opt_in, Some("1" | "true" | "yes"))`; unit test `release_debug_server_requires_explicit_opt_in`.
- `grep -c BYRO_DEBUG_SERVER` = 0 for capture.sh, m34-day-night.sh, m-trees.sh, m43-quest-runtime.sh, m47-triggers.sh, r6a_stale_15_bench.sh, scripts/material-provider-matrix.sh — each launches `--release`.
- m48-* scripts: `OUT_DIR="$(mktemp -d)"` then `screenshot $OUT_DIR/…png`.
- Runtime audit: first FNV attempt at HEAD, log `/tmp/audit/runtime/fnv-attempt1-no-optin.engine.log` (`bench-hold-unavailable` → `capture: FATAL debug server never came up`); re-running the unmodified script with `BYRO_DEBUG_SERVER=1` in the environment succeeded (`dbg up at 1s`).

## Impact
All of these manual acceptance gates are permanently red, so they carry no signal: the runtime telemetry audit, the only captured-frame gates for sky assembly (cycle), water shading (water) and the Skyrim sun response (m34), the only runtime SpeedTree gate (m-trees), the M48.4–M48.7 HUD pixel gates (Oblivion, FO3, FNV, Skyrim, FO4), the M43 quest-runtime and M47 trigger gates, and the R5.5 material-provider capture matrix. #4898 still needs a captured distant-terrain frame. A developer who follows CLAUDE.md's usage line gets a byro-dbg "Failed to connect" and no hint why. The CI arm (`m-exteriors static`, `playable-smoke.yml`) uses the CLI `--screenshot` path and is unaffected, which is why nothing noticed.

## Related
RT-1 (AUDIT_RUNTIME_2026-09-29), EXT-D7-2026-09-29-01 (AUDIT_EXTERIOR_2026-09-29), SPT-D3-01 (AUDIT_SPEEDTREE_2026-09-29) — all folded into this issue. #4752 (the fix that introduced both rules), #4724 (HUD smokes `exit 1` on missing data), #4491. Label gap: debug server / byro-dbg / audit infrastructure have no own label → `tech-debt`.

## Suggested Fix
- **Opt-in:** add `BYRO_DEBUG_SERVER=1` (and `BYRO_DEBUG_PORT="$PORT"`) to the release launches in capture.sh, m34, m-trees, m43, m47, r6a and material-provider-matrix. In capture.sh, also fail fast when the engine log shows `bench-hold-unavailable` instead of waiting out the 90 s loop, and mention the opt-in in the audit-runtime SKILL.md invocation paragraph.
- **Screenshots:** capture by bare filename and `mv` the file from `screenshots/` into the profile/out dir, as `p3-hud.sh` does — m-exteriors (12 sites), m48-4..7 HUD scripts, material-provider-matrix.
- **m-trees:** fail loudly when byro-dbg output lacks the `entities` response instead of parsing it as 0; add an `obl` arm (e.g. a Cyrodiil exterior grid with `Oblivion - Meshes.bsa` + `Oblivion - Textures - Compressed.bsa`).
- **r6a:** fix the `cd "$game_data"` launch and the ignored `-p` argument.
- **Docs:** add the opt-in and bare-filename rule to CLAUDE.md, README.md, docs/engine/testing.md and docs/smoke-tests/README.md.
- **Guard:** a static test that scans `docs/smoke-tests/*.sh`, `scripts/*.sh` and `.claude/commands/audit-runtime/capture.sh` for a `screenshot <path containing />` line and for a release byro-dbg harness without the opt-in (plus a capture.sh self-test/static pin that the launch line carries it).

Validated at HEAD 9fcfdc3fc: `debug_server_allowed` and `write_screenshot` unchanged; zero `BYRO_DEBUG_SERVER` hits in the seven launchers listed in A; m-exteriors/m48-*/material-provider-matrix still send directory paths; CLAUDE.md/README.md/testing.md/smoke README carry no opt-in.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every `docs/smoke-tests/*.sh`, `scripts/*.sh`, `.claude/commands/*/` harness that drives byro-dbg)
- [ ] **TESTS**: A regression test pins this specific fix (static harness scan)
