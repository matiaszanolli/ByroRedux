==================== #5121 [OPEN] SAFE-D4-2026-09-29-02: CI clippy is red on rustc 1.98.1 in `byroredux-sdk` and `byroredux-nif`, so the run aborts before the renderer is checked; `#![deny(clippy::undocumented_unsafe_blocks)]` is enforced nowhere in CI (regression of #4595)
labels: ['bug', 'renderer', 'medium', 'safety', 'tech-debt']
**Source report**: `docs/audits/AUDIT_SAFETY_2026-09-29.md`

**Regression of #4595** (#4595 is CLOSED; filed as a new issue rather than reopening it).

- **Severity**: MEDIUM. This is the same class as SAFE-D4-2026-09-21-01 (#4595): the only mechanical guard on the renderer's 733 `unsafe` blocks is off.
- **Dimension**: Unsafe-Block Discipline
- **Location**:
  - `.github/workflows/ci.yml:130` (`dtolnay/rust-toolchain@stable`, unpinned) and `:177-179` (`cargo clippy --workspace -- -D warnings`, no `--keep-going`);
  - the new-lint sites `crates/sdk/src/event.rs:447` and `crates/nif/src/{anim/controlled_block.rs:97, blocks/bs_geometry.rs:442/455/462, blocks/legacy_particle.rs:687, blocks/node.rs:1231, blocks/skin.rs:502, import/mesh/bs_tri_shape.rs:193, import/mesh/normal.rs:127, import/mesh/skin.rs:75, import/types.rs:1531}`.
- **Status**: Regression of #4595 (CLOSED 2026-09-22). #4595's remedy still holds: clippy runs even when tests are red. But the gate it restored no longer reaches the renderer. #4765 (CLOSED 2026-09-29, 554ef5c44) greened clippy on the local 1.96 toolchain only. No open issue covers this.
- **Description**:
  - CI installs `stable`, currently **rustc 1.98.1** (`48a229cea 2026-09-01`). The workstation runs 1.96.0, and the repo has no `rust-toolchain.toml`.
  - Clippy 1.98 adds `chunks_exact_to_as_chunks` and fires `question_mark` on one more pattern, giving 1 error in `byroredux-sdk` and 11 in `byroredux-nif`.
  - Cargo stops scheduling once those crates fail. The log shows `Compiling byroredux-renderer` (its build script) but never `Checking byroredux-renderer`, so the renderer's crate-level `#![deny(clippy::undocumented_unsafe_blocks)]` (`crates/renderer/src/lib.rs:21`) is never evaluated.
  - The clippy step has concluded `failure` on every main run in the sampled history, from `839b8dcea` (2026-09-25T14:08Z) through HEAD. The per-run causes before HEAD were not individually read; at HEAD, after #4765 landed, the only errors are the 12 rustc-1.98 lint sites below.
- **Evidence**: HEAD run `36609043348`, job `109545443996`, step "cargo clippy":
  ```
  error: using `chunks_exact` with a constant chunk size
     --> crates/sdk/src/event.rs:447:23
  error: could not compile `byroredux-sdk` (lib) due to 1 previous error
  error: this block may be rewritten with the `?` operator
     --> crates/nif/src/anim/controlled_block.rs:97:16
  … (10× chunks_exact_to_as_chunks in byroredux-nif)
  error: could not compile `byroredux-nif` (lib) due to 11 previous errors
  ```
  The same step printed `Checking` lines for 15 workspace crates (plus `Compiling` for the build scripts of fsr3-sys, cxx-bridge and renderer), and `byroredux-renderer` is never `Checking`.
- **Impact**: A comment-less `unsafe {}` added to the renderer today would pass CI. The local lint run above shows the renderer is clean at HEAD, so no such block exists yet. Every other clippy-enforced invariant in crates downstream of `sdk`/`nif` is also unchecked, and the job is permanently red, which trains readers to ignore it.
- **Related**: #4595, #4765, #4567 (the previous toolchain-bump red), SAFE-D4-2026-09-21-01. Memory note *Clippy --keep-going*: workspace clippy aborts at the first failing crate.
- **Suggested Fix**:
  - Fix the 12 sites. `as_chunks::<N>().0` is the suggested rewrite; `?` for `controlled_block.rs:97`.
  - Stop a toolchain bump from silently shadowing the renderer gate, by either:
    - pinning the CI toolchain with a `rust-toolchain.toml` that the workstation shares; or
    - adding `--keep-going` plus a dedicated `cargo clippy -p byroredux-renderer --no-deps -- -D clippy::undocumented_unsafe_blocks` step that cannot be pre-empted by an unrelated crate.

**Validated at HEAD 9fcfdc3fc**: HEAD CI run 36609043348 job "Test + Check + Clippy" concluded `failure`; its log shows the 1 `byroredux-sdk` error (`crates/sdk/src/event.rs:447`) and 11 `byroredux-nif` errors at the listed sites; `.github/workflows/ci.yml` uses `dtolnay/rust-toolchain@stable` and `cargo clippy --workspace -- -D warnings` without `--keep-going`; no `rust-toolchain.toml` exists in the repo.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix


==================== #5142 [OPEN] TOOL-D1-2026-09-29-03: `63c0aee3b`'s release debug-server opt-in and bare-filename screenshot rule broke every byro-dbg harness that predates it — capture.sh, m-exteriors, m34, m-trees, m43, m47, m48-4..7 HUD, r6a, material-provider-matrix, plus 4 docs
labels: ['bug', 'medium', 'tech-debt', 'doc-rot', 'test-gap']
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


==================== #5005 [OPEN] FNV-2026-09-29-D2-01: FO3/FNV authored corpses are marked by XRGD ragdoll data, not a header flag — #4814's Starts Dead decode never fires, so every placed corpse spawns alive
labels: ['bug', 'high', 'legacy-compat', 'gameplay', 'game:fnv', 'game:fo3', 'esm-plugin']
**Source report**: `docs/audits/AUDIT_FNV_2026-09-29.md` (FO3 evidence: `docs/audits/AUDIT_FO3_2026-09-29.md`, FO3-D2-01)
**Severity**: HIGH (same impact class as #4814)
**Dimension**: ESM Data Slice. Decode belongs to `/audit-esm`, consumption to `/audit-gameplay`.

## Location
- `crates/plugin/src/esm/cell/walkers.rs` (`PlacedRef` construction): `starts_dead = ACHR && EsmVariant::Tes5Plus && flags & FLAG_STARTS_DEAD`.
- `crates/plugin/src/esm/reader.rs` (`FLAG_STARTS_DEAD = 0x200`, cited from xEdit's TES5 list only).
- `crates/plugin/src/esm/cell/mod.rs` (`PlacedRef::starts_dead` field doc: "FO3/FNV ship none").
- `byroredux/src/cell_loader/references/mod.rs` (actor-job completion) → `byroredux/src/cell_loader/reference_state.rs` (`apply_starts_dead`), the only consumer.

## Description
#4814 (CLOSED) decodes "Starts Dead" only as the ACHR header bit `0x200`. xEdit's FO3/FNV definitions (`wbDefinitionsFNV.pas` / `wbDefinitionsFO3.pas`) give ACHR the record flags {10 Persistent, 11 Initially Disabled, 25 No AI Acquire}, and ACRE adds 15 Visible When Distant. Neither record type has a Starts Dead bit. Both carry `wbRagdoll` (`XRGD` / `XRGB`).

FO3/FNV placed corpses therefore carry no header signal. What they carry is an authored ragdoll pose (`XRGD`). Nothing in the engine reads `XRGD` (0 code hits in `crates/` and `byroredux/`), so `starts_dead` is false for every FO3/FNV actor. These actors go through the normal actor job, get their base's packages and ambient AI, and hostile ones are armed by #4414's faction hostility.

**The rule is inferred from data, not sourced.** "`XRGD` present on an ACHR/ACRE means the actor starts dead" comes from the census below plus xEdit's flag lists. No xEdit or GECK text documents it as "starts dead" (the GECK wiki is Cloudflare-blocked). Per the No Guessing policy, a source must be found before the fix ships.

## Evidence
Byte scan of the vanilla masters (24-byte group headers, compressed bodies inflated).

**FalloutNV.esm**
- Header flag `0x200`: 0 of 3,386 ACHRs and 0 of 2,999 ACREs.
- 387 actor refs carry `XRGD` (278 ACHR, 109 ACRE). 343 of the 387 place a base whose EditorID contains `dead` or `corpse` (`VHDDeadNCRTrooper`, `VHDDeadLegionary`, `VHDDeadCenturion`, `NVProspectorMaleDEADLite`, `NVProspectorMaleDEADSulfurCave03`). Most of the other 44 are deactivated robots (`SSHQProtectronBroken`, `SSHQMrHandyBroken`). Only 14 non-`XRGD` refs place a Dead-named base.
- Header flags on the 387: `0x0` ×209, `0x400` ×146, `0xC00` ×19, `0x800` ×10, so about 358 are not initially disabled and spawn at cell load.
- 29 use bases scripted with `VHDDeadSafetySCRIPT` (`Begin OnLoad / If (GetDead == 0) / Kill`), a safety net for an actor that is normally already dead. The engine has no object-script `Kill` either.

**Fallout3.esm** (from FO3-D2-01)
- `0x200`: 0 of 2,154 ACHR and 0 of 3,349 ACRE.
- 498 actor refs carry `XRGD` (338 ACHR, 160 ACRE); every payload is a multiple of 28 bytes (per-bone pose).
- 428 of the 435 refs whose base is named dead or loot-corpse carry `XRGD` (`DeadBrahmin` ×35, `DeadMoleRat` ×17, `FeralGhoulDEAD` ×14, `DEADGhoulWastelander*` ×24, `DeadSuperMutant1Gun*` ×15, `Loot1*` ~170). The other 70 also read as corpses (`MS16Corpse2/4`, `OasisCorpse`, `AndaleVictim01-03`, `MS18WreckedProtectron01`).
- Base AI is Very Aggressive on 338 of the 498 and Aggressive on 73.

Validated at HEAD 9fcfdc3fc: `walkers.rs` sets `starts_dead` only for `ACHR` + `Tes5Plus` + `0x200`; `rg XRGD crates byroredux` returns 0 hits; the `PlacedRef::starts_dead` doc still says "FO3/FNV ship none".

## Impact
- About 885 authored corpses (387 FNV + 498 FO3) spawn as live NPCs running their packages: the Hoover Dam battlefield dead (Legion corpses are hostile), the Prospector corpses in the caves, the broken Securitron HQ robots, FO3 dead brahmin/ghouls/super mutants and loot corpses.
- They cannot be looted, because `is_loot_source` is gated on `Dead`.
- A hostile corpse starts combat on sight (#4414).
- The #4814 field doc tells the next reader that FO3/FNV have no corpses, which is false.

## Related
- #4814 (CLOSED, Skyrim/FO4 header bit; explicitly scoped FO3/FNV out), #4817, #4693, #4772.
- Sibling per-game decode gaps from the same 2026-09-29 suite (each a distinct marker): see the cross-reference section below.
- AUDIT_ESM_2026-09-29 notes the `0x200` gate is inert on FO3/FNV but does not identify the FO3/FNV marker.

## Suggested Fix
1. First confirm the engine rule (No Guessing policy): find a GECK/xEdit/engine source that `XRGD` presence marks a placed actor as dead on FO3/FNV.
2. Decode `XRGD` presence on ACHR/ACRE for the FO3/FNV variant into `PlacedRef::starts_dead` (or a sibling field). Keep the `0x200` path for Skyrim and later.
3. Correct the field doc.
4. Add real-master census guards (`FalloutNV.esm` `XRGD`-actor count ≥ 380; `Fallout3.esm` = 498).
5. Later, consume the `XRGD` pose as the corpse's initial ragdoll pose (shared with the Skyrim/FO4 pose issue below).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (ACHR and ACRE; FO3 and FNV; the Skyrim/FO4 `0x200` path stays intact)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic ACHR/ACRE with `XRGD`, plus a real-master census guard)

## Cross-references: the starts-dead / starts-unconscious decode family (2026-09-29 suite)
#4814 only knows the Skyrim/FO4 ACHR header bit. Each game below has a distinct marker, so each is filed separately:
- #5005 — FO3/FNV: `XRGD` presence is the only corpse marker (rule **inferred from data, needs a source before fixing**).
- #5013 — Oblivion: base `NPC_`/`CREA` header flag `0x80000` + 0 Health (sourced: xEdit TES4, CS wiki).
- #5015 — Skyrim/FO4: the `XRGD` corpse pose is never decoded (marker is correct; pose is lost).
- #5017 — FO4: ACHR `0x2000` "Starts Unconscious" undecoded (dormant semantics **inferred, needs a source before fixing**).


