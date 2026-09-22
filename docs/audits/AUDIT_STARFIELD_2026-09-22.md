# Starfield Compatibility Audit — 2026-09-22

**HEAD**: `ee6d3fb39` · **Baseline**: `docs/audits/AUDIT_STARFIELD_2026-09-16.md` (HEAD
`7996edf61`) · **Audited**: Dimensions 3, 4, 6 (commits since baseline) · **Unchanged
since baseline (skimmed, guard spot-checked)**: Dimensions 1, 2, 5

**Method**: One agent (no nested sub-agents, per this run's constraints) read live
source at HEAD with delta-first scoping: `git log --since=2026-09-16` against each of
the six dimensions' `Paths:` lists determined that Dimensions 1 (BA2 v2/v3), 2
(BSGeometry) and 5 (NIF shader blocks) had zero commits since baseline and were
guard-spot-checked only (unit tests re-run, no new code read line-by-line); Dimensions 3
(CDB), 4 (ESM/cell bring-up) and 6 (material flow/NIFAL) had commits and were read and
verified in full, including a byte-consumption-parity review of the new CDB streaming
reader's `consume_*`/`skip_*` function pairs. The engine binary was not launched and no
`--ignored` real-data corpus test was re-run this cycle (resource constraints — the
baseline's 100.00%/120,543-NIF parse-rate figure and the ~1.44M-instance CDB structure
are unaffected by any commit since baseline, so citing rather than re-measuring is safe).
`gh issue list` was refreshed to 4,569 issues (max #4681) and used for dedup.

**Result**: **0 CRITICAL, 0 HIGH, 0 MEDIUM, 1 LOW**. The one finding is NEW. Zero
regressions. Notably, **5 of the 9 findings from the 2026-09-16 baseline report are now
CLOSED and verified fixed in code** (D3-02, D3-03, D4-01, D5-01, META-01). The other
four (D3-01/#4429, D6-01/#4439, D7-01/#4440, D8-01/#4441) remain open, confirmed
unaddressed this cycle and re-cited rather than re-reported — D3-01 is the multi-step
canonical texture-role prerequisite and is open by design, not a quick fix; the other
three are doc-accuracy items with no commits to their cited text.

---

## Executive Summary

Starfield remains a first-class `GameKind` with no regressions this cycle. The prior
audit's premise correction — that vanilla Starfield's CDB `.mat` material path produces
**zero** texture roles today (presence-only, #4429/#3398 pending) — still holds; nothing
this cycle changed the canonical role vocabulary or the CDB Phase-2 indexed reader. What
*did* land between 2026-09-16 and HEAD is real forward progress on several fronts this
audit verified line-by-line:

- **A genuine visual-correctness fix**: Starfield placed `LIGH` lights previously
  rendered universally as omnidirectional points. `6c45c5bde` decoded the newly-published
  xEdit `DAT2+56` "Light Type" enum and wired it through `translate_light` /
  `canonical_light_shadow_flags`, correctly distinguishing Omnidirectional / Shadow
  Spotlight / NonShadow Spotlight. Verified bounds-checked, correctly gated by `game`,
  and covered by new targeted tests (`starfield_light_type_enum_drives_the_spot_shape`,
  `starfield_shadow_technique_follows_the_light_type_enum`), all green.
- **CDB reader hardening toward Phase 2**: a new streaming visitor
  (`visit_instances_with_limits`) and streaming validator
  (`validate_instances_with_limits`, closing #4274's 9.19 GB real-data test problem) were
  added. This audit cross-checked every `consume_*`/`skip_*` function pair in
  `crates/sfmaterial/src/reader.rs` for byte-consumption parity (the CDB analogue of a
  NIF stream-position mismatch) and found none — the two readers walk identical chunk
  structure. Nothing yet builds the `material_path → MaterialFields` index Phase 2 needs;
  this is scaffolding, not the index itself.
- **Doc-rot cleanup**: the CDB Phase-2 spike's dead example references (#4435), its
  wrong-sink/closed-tracker doc (#4436), the pre-split ESM coverage doc pointers (#4437),
  and the `slot_to_colocated_role` Starfield/Skyrim vocabulary collision (#4431) are all
  fixed and tested.
- **A caught-and-fixed dark test**: `#4579` found and fixed an orphaned `#[test]`
  attribute (introduced by `c0b740ce7` two commits earlier in the same cycle) that had
  silently stopped running the NIFAL boundary's core copy-fidelity guard
  (`translate_material_copies_every_canonical_field`). Confirmed fixed with a structural
  meta-guard against recurrence, and confirmed green.

Four genuinely open items remain exactly where the baseline left them, confirmed
unaddressed this cycle: the canonical texture-role vocabulary gap for roughness /
metalness / AO / opacity / transmissive (#4429, ~39% of Starfield's own texture corpus),
the #1510 "content-hash path" premise still asserted without vanilla evidence (#4439),
and two doc-accuracy items (#4440: ROADMAP still says 99.98% instead of the measured
100.00%; #4441: `translate_material`'s own contract doc still misdescribes BGEM as
pre-classified).

**Walkable Cydonia**: not re-verified this cycle (no engine launch permitted under this
run's constraints); #3540's frame-0 stall fix remains unconfirmed on real device, as at
baseline.

---

## Dimension Findings

### Dimension 1 — BA2 v2/v3 + Corpus Validation
**Verdict**: Unchanged since baseline. Zero commits to `crates/bsa/src/ba2.rs` or the
parse-rate harness since 2026-09-16. Guard spot-check: `cargo test -p byroredux-bsa --lib
-- ba2` — **35/35 passed**, including every LZ4/v3-dispatch guard the skill names. The
100.00%/120,543-NIF corpus figure (measured at baseline) was not re-run this cycle
(resource constraints; nothing could have moved it). No findings.

### Dimension 2 — BSGeometry Mesh Extraction
**Verdict**: Unchanged since baseline. Zero commits to `bs_geometry.rs` or
`skeleton.rs`. Guard spot-check: `cargo test -p byroredux-nif --lib -- bs_geometry` —
**68/68 passed** (same count as baseline). No findings.

### Dimension 3 — CDB Material Database
**Verdict**: 0 findings. Two baseline LOW findings (D3-02, D3-03) closed and verified
fixed. A significant new feature (the streaming visitor/validator, #4274) landed and was
read in full; no byte-consumption divergence found between the new skip-path and the
existing materializing-path readers. `cargo test -p byroredux-sfmaterial --lib` —
**26/26 passed**; `cargo test -p byroredux --bin byroredux -- starfield_mat` — **13/13
passed**. Two FO4-scoped BGSM/BGEM merge-arm fixes (#4402, #4425) checked for Starfield
spillover and confirmed inert (Starfield ships zero real `.bgsm`/`.bgem` files, so those
arms never execute for Starfield content). #4429 (the canonical texture-role vocabulary
gap) confirmed still open and unaddressed — no commits to `types.rs` or `material.rs`
add the missing roles. Full detail: `/tmp/audit/starfield/dim_3.md`.

### Dimension 4 — ESM Resolve Rate + Cell Bring-up
**Verdict**: 1 LOW (NEW). Both baseline findings (D4-01, D5-01) closed and verified
fixed. The Starfield `LIGH` spotlight fix (`6c45c5bde`) and its falloff-exponent /
XCLL-ambient-cube-game-validation companions (`6b4e6252c`, `efe15ceaf`) were read in full
and verified correct: bounds-checked DAT2+56 read, correctly `game`-gated shape/shadow
logic, Starfield's dedicated ≥108-byte XCLL arm confirmed untouched by the new
≥92-byte-arm game restriction. `cargo test -p byroredux-plugin --lib -- starfield xcll
txst dat2` — **50/50 passed** (3 ignored, need real game data); `cargo test -p byroredux
--bin byroredux -- light_anim` — **28/28 passed**.

#### SF-2026-09-22-D4-01: `warn_unmodelled_txst_slot`'s regression guard depends on being the sole owner of a process-global, unresettable seen-set
- **Severity**: LOW
- **Dimension**: 4 (ESM + cell bring-up)
- **Location**: `crates/plugin/src/esm/cell/support.rs:698-711`
  (`warn_unmodelled_txst_slot`); `crates/plugin/src/esm/cell/tests/txst.rs:654-697`
  (`starfield_pbr_slots_parse_without_dropping_the_record`)
- **Status**: NEW
- **Description**: `warn_unmodelled_txst_slot`'s first-sighting gate (added by #4438) is
  a `static OnceLock<Mutex<HashSet<[u8;4]>>>` scoped to the whole test/production
  process, with no test-only reset. Its regression test asserts directly on this global
  state, including an assertion that `TX17` is seen for the first time ever
  (`assert!(warn_unmodelled_txst_slot(b"TX17"))`). That holds today only because no
  other test in the crate touches `TX08`/`TX09`/`TX17`/`TX19`.
- **Evidence**: `grep -rn '"TX17"\|b"TX17"\|TX08\|TX09\|TX19' crates/plugin/src/esm/cell/tests/ crates/plugin/src/esm/records/`
  finds no other reference. The test's own comment reasons only about ordering *within
  its own body*, with no guard against a different test running first in the same
  process.
- **Impact**: Latent. A future TXST test touching any of these four FourCCs (e.g. the
  real-data `Starfield.esm` TXST census the module doc still lacks) will make this
  test's outcome depend on `cargo test`'s thread-scheduling order — an intermittent
  failure with no message pointing at the real cause.
- **Related**: #4438 (the fix this guards).
- **Suggested Fix**: Add a `#[cfg(test)]`-only reset function and call it at the top of
  the test, or use FourCCs reserved exclusively for tests instead of real TXST slot
  names.

Full detail: `/tmp/audit/starfield/dim_4.md`.

### Dimension 5 — NIF Shader Blocks, BSVER 155+
**Verdict**: Unchanged since baseline. Zero commits to `crates/nif/src/blocks/shader/`
or `shader_flags.rs`. Guard spot-check: `cargo test -p byroredux-nif --lib --
starfield` — **27/27 passed**, spanning tail-capture, CRC32-driven decal/two-sided, and
slot-vocabulary-parity guards. #4439 (SF-2026-09-16-D6-01, content-hash-path premise)
confirmed still open and unaddressed — the two cited comments are byte-identical to the
baseline citation. No findings.

### Dimension 6 — Material Flow — NIFAL Boundary + BGSM/BGEM/`.mat`
**Verdict**: 0 findings. #4431 (`slot_to_colocated_role` grouping Starfield with Skyrim)
verified fixed and correctly scoped, with extended test coverage across every
non-Skyrim layout. The new `_msn`-suffix model-space-normal rule (#4548) was checked for
unintended Starfield spillover — it cannot fire today because Starfield resolves zero
texture roles from any source, so `textures.normal` is always `None`; worth re-checking
once CDB Phase 2 lands per-texture extraction. #4579 (a dark test caught and fixed
within this same cycle, before this audit's window) verified restored and green.
`cargo test -p byroredux-nif --lib -- slot_role` — **20/20 passed**; `cargo test -p
byroredux --bin byroredux -- material_translate` — **67/67 passed**. Full detail:
`/tmp/audit/starfield/dim_6.md`.

---

## CRC32 Flag Table

No changes since `AUDIT_STARFIELD_2026-09-16.md` — zero commits to
`crates/nif/src/shader_flags.rs` or the shader-block family this cycle (Dimension 5,
unchanged). The derivation and the full 32-row table are unchanged; see the 2026-09-11
and 2026-09-16 reports for the complete listing. #4279 (`Own_Emit` additive-blend
promotion typed-word-only) remains OPEN, unaffected.

---

## Remaining-Work Chain

Unchanged in shape from the baseline; only the "solved this cycle" annotations move:

1. **CDB Phase 2: per-field `.mat` extraction** (#3398, OPEN). What remains:
   - **(a) The canonical texture-role vocabulary** for roughness / metalness / AO /
     opacity / transmissive (#4429, MEDIUM, OPEN, confirmed unaddressed this cycle).
   - **(b) The indexed or streaming reader** — partially advanced this cycle:
     `visit_instances_with_limits` / `validate_instances_with_limits` (#4274, closed)
     give Phase 2 a memory-safe way to walk the corpus, but the actual
     `material_path → MaterialFields` index is not yet built.
   - **(c) Restoring the key-derivation probe** — done this cycle (#4435).
2. **PDCL ahead of GBFM** (unchanged — PDCL 706 records / 74.9% of unresolved Cydonia
   REFRs vs. GBFM 3,141 records / 0.081%).
3. **Exterior worldspace tiles** — unblocks the LTEX/TXST consumers TXST's four
   unmodelled PBR slots (#4438, now at least visible via warn-once) feed.
4. **Space-cell / planet / GBFM records**: SFTR, PNDT, STDT, BIOM still `skip`.
5. The #2105/#3524 NIF truncation tail: cleared, unchanged since baseline (100.00%
   clean); only the docs (#4440) still need to catch up.

Both the BGSM parser and the ESM parser have shipped; this is the accurate ordering.

---

## Coverage Notes

- **Not re-run this cycle**: `--sf-smoke` (engine-binary-gated, forbidden under this
  run's constraints), the full real-data `parse_rate_starfield_all_meshes -- --ignored`
  corpus gate, and the `#[ignore]`d `real_cdb.rs` streaming-validator test against the
  actual ~1.44M-instance vanilla CDB. None of the commits since baseline touch the code
  paths these exercise in a way that would move their measured figures, so citing the
  2026-09-16 figures is sound; none should be treated as freshly re-verified.
- **Un-owned subsystems**: unchanged from baseline (Gameplay, SDK, Launcher, FaceGen,
  Mod Runtime, FSR3 FFI, Havok packfile reader, Debug server) — none Starfield-specific.
- **Scratch tooling**: all work this cycle happened in `/tmp/audit/starfield/` (six
  `dim_N.md` files) plus read-only `cargo test`/`cargo check`/`git show` invocations. No
  source, test, doc or skill files were modified.

## Total Findings Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 1 |
| **Total NEW** | **1** |

| ID | Sev | Title | Status |
|---|---|---|---|
| SF-2026-09-22-D4-01 | LOW | `warn_unmodelled_txst_slot`'s regression guard depends on sole ownership of an unresettable global seen-set | NEW |

**Matched-existing (still open, cited, not re-reported)**: #3398 (MEDIUM), #4429
(MEDIUM), #4279 (MEDIUM), #4283 (MEDIUM), #4268 (MEDIUM), #4282 (MEDIUM), #3659
(MEDIUM), #4277 (LOW), #4285 (LOW), #4439 (LOW), #4440 (LOW), #4441 (LOW), #4269 (LOW).

**Closed since baseline, verified fixed in code**: #4435, #4436, #4437, #4438, #4446
(the baseline report's own D3-02/D3-03/D4-01/D5-01/META-01 findings); plus #4402, #4425,
#4431, #4548, #4579 — issues filed by sibling audits (NIFAL/FO4/renderer) against shared
mechanisms, checked here for Starfield spillover and confirmed either inert (FO4-scoped
BGSM/BGEM arms Starfield never reaches) or a genuine, correctly-scoped fix
(`slot_to_colocated_role`, the LIGH DAT2 companions, the restored dark test). (#4272,
#4273, #4275, #4278, #4284 and #4394 were already closed as of the 2026-09-16 baseline
and are not re-counted here.)

Suggested next step: `/audit-publish docs/audits/AUDIT_STARFIELD_2026-09-22.md`.
Label the one new finding `game:starfield` + `legacy-compat` + `esm-plugin` + `test-gap`.
