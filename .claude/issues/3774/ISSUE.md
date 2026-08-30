# #3774 — UI-D4-2026-08-30-05: docs/engine/ui.md has re-drifted from the crate in three places, including an advertised UiManager::close() that no longer exists

**Repo**: matiaszanolli/ByroRedux · **Filed**: 2026-08-30 · **HEAD**: `64f64480`
**Labels**: low, ui, doc-rot, documentation

---

**Audit**: `/audit-ui` — `docs/audits/AUDIT_UI_2026-08-30.md` (Dimension 4 — Catalog Fidelity & Drift), HEAD `64f64480`
**Finding ID**: `UI-D4-2026-08-30-05`

- **Severity**: LOW
- **Status**: NEW

## Location

`docs/engine/ui.md` — `:252`, `:75-76`, `:66-83`, `:89-116`, `:554`

## Description

`docs/engine/ui.md` is the authoritative host-contract doc `/audit-ui` sends every auditor to read first. Three claims in it are false at HEAD.

**1. `:252` declares `pub fn close(&mut self);`** in the `UiManager` API block — "drops the player, clears state". That method was **deleted** under #2723; `grep -rn 'fn close' crates/ui/src/` returns nothing, and `lib.rs:288-296` carries the comment explaining the removal. The doc advertises API that does not compile. The module-map blurb at `:75-76` repeats it ("load/tick/render/**close**").

The same API block also omits `load_swf_with_profile`, `host_bridge`, `drain_host_calls`, `dropped_host_calls` and `invoke_callback`, all `pub`.

**2. The module map at `:66-83` lists 8 modules; the crate has more.** `crates/ui/src/prepare.rs` (#2968, `0e91fc5e`) is absent, and the Pipeline diagram at `:89-116` still begins at `SwfMovie::from_data` with no mention of `prepare_movie` — so the one-decompress/one-tag-walk invariant that module exists to hold is invisible in the doc. (`ls crates/ui/src/` at HEAD also shows a `host/` directory beside `host.rs`, likewise unlisted.)

**3. `:554` says "53 default tests plus 2 ignored".** Measured at HEAD with the two commands the doc itself names: `cargo test -p byroredux-ui -- --list` → 61 total; `-- --list --ignored` → 2. So **59 default plus 2 ignored**. The +6 are `prepare::tests` ×4, `lib::tests::the_frame_driver_reads_the_drop_counter_beside_the_drain`, and one `host::tests` addition from `a984836c`.

## Evidence

Re-verified at HEAD `64f64480`:
- `grep -n 'pub fn close' docs/engine/ui.md` → `:252`; `grep -rn 'fn close' crates/ui/src/` → nothing
- `sed -n '66,84p' docs/engine/ui.md` lists 8 entries; `ls crates/ui/src/` shows `avm2_host.rs catalog.rs host/ host.rs input.rs lib.rs navigator.rs player.rs prepare.rs profile.rs`
- `sed -n '554p' docs/engine/ui.md` → "The UI crate has **53 default tests plus 2 ignored** installed-corpus smokes"

## Impact

Claim (3) is the same rot #3272 closed on 08-26; the line even carries its own warning — *"re-measure rather than trusting this line — it has drifted four times"* — now five.

Claims (1) and (2) are new this cycle and are the more damaging: **(1) is an API contract a reader would act on**, and (2) hides the module whose whole purpose is holding the decode-count invariant `UI-D1-2026-08-30-01` (#3771) is about.

## Related

- #2723 (deleted `UiManager::close`)
- #2968 / `0e91fc5e` (added `prepare.rs`)
- #3272 (the previous test-count rot on the same line)
- #3433 (the same file's 6-attachment render-pass claim — OPEN; worth one edit pass together)

## Suggested Fix

One edit pass: delete the `close()` line and the `/close` in the module-map blurb, add `prepare.rs` (and `host/`) to the module map and `prepare_movie` to the pipeline diagram, add the five omitted `pub` methods to the API block, and re-measure the counts.

Doc rot inherits `/audit-tech-debt` Dim 3's severity floor: LOW.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — the other `docs/engine/ui.md` claims tracked by #3433, and the module-map blurb at `:75-76`
- [ ] **TESTS**: N/A (documentation) — but consider a source-scan guard that fails when the module map and `ls crates/ui/src/` disagree, since this line has now drifted five times
