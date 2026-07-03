# Legacy Compatibility Audit — 2026-07-03

**HEAD:** 8498e559 · **Suite:** comprehensive (1 of 21, run independently)

**Scope:** Compatibility/mapping gaps between Gamebryo 2.3 / Creation-engine
behaviour and Redux's current implementation, framed by the three canonical
translation layers (NIFAL / EXAL / PHYSAL) plus coordinate-system correctness,
the per-game translation survey, and subsystem coverage vs. the legacy headers.

**Method:** This is a **delta pass** over the exhaustive 2026-07-02 audit
(`AUDIT_LEGACY-COMPAT_2026-07-02.md`), which ran at essentially the same tree and
verified all four single-producer boundaries clean. Since that report, four
commits landed (`ffe9a816` #1718 ragdoll telemetry, `175ebf2c` #1731 VWD flag
parse, `ae219630`/`2f0b99fa` PEX tests, `8498e559` skill refresh). I re-ran the
boundary single-producer greps, re-verified the two closed findings' actual
scope, deduplicated every candidate against the 71 open issues
(`/tmp/audit/issues.json`) and the per-layer specs, and attempted to disprove
each finding before inclusion.

**Headline:** All four canonical boundaries (material / env / coord / ragdoll)
remain single-producer clean and the renderer carries zero per-game branches. The
one substantive delta: #1731 closed at **parse** scope (the VWD / "Has Distant
LOD" record-header flag is now parsed and test-pinned) but the runtime
**consumer** — culling the full model once its LOD proxy is active — was deferred
and is now tracked by **no** open issue. That untracked coverage gap is the single
NEW finding.

---

## Boundary verification results (no findings — recorded for the trail)

| Layer | Claim verified | Result |
|---|---|---|
| **Coordinate system** | `(x,z,-y)` swap + `EXTERIOR_CELL_UNITS` single-source | **Clean.** No duplicated axis-swap or raw `4096.0` cell literal outside `crates/core/src/math/coord.rs`. All `4096.0` hits are comments (coord.rs / scene_buffer/constants.rs / esm/cell/mod.rs), test coords (draw.rs:3911), or distinct constants (fog_volume.rs far-plane default). |
| **NIFAL — material** | `translate_material` is sole populated-`Material` producer | **Clean.** Only non-test callers: `byroredux/src/scene/nif_loader.rs:828` and `byroredux/src/cell_loader/spawn.rs:880`. `asset_provider/material.rs:387` builds `byroredux_bgsm::template::ResolvedMaterial` (distinct BGSM type); `helpers.rs`/`cornell.rs` are reference scenes. No `metalness_override`/`classify_pbr` render-time reappearance. |
| **EXAL — env resources** | `env_translate.rs` sole `SkyParamsRes`/`WeatherDataRes` producer | **Clean.** Non-`env_translate` construction sites (`systems/weather.rs:1210`, `render/lights.rs:253`) are inside `#[cfg(test)]` modules (weather.rs:1210 sits below the `#[cfg(test)]` at line 1153). |
| **PHYSAL — ragdoll** | one translate (`template_from_imported`/`activate_ragdoll`), one sink (`build_ragdoll`) | **Clean.** `extract_ragdoll` has no `game ==` branch; zero Rapier types leak outside `crates/physics`. |
| **Renderer per-game branch** | render/renderer side carries no `if game == …` | **Clean.** Grep for `game ==` / `GameKind::` / `is_skyrim` / `is_fo4` across `byroredux/src/render` + `crates/renderer/src` → zero hits. Leaks (if any) live upstream in parser/importer, per the survey. |

---

## Findings

### LC0703-01: VWD "Has Distant LOD" full-model cull consumer is now untracked
- **Severity**: MEDIUM
- **Dimension**: EXAL — LOD distance rendering (SKILL Dimension 5)
- **Location**: `crates/plugin/src/esm/reader.rs:384` (`is_visible_when_distant`, producer), `byroredux/src/cell_loader/object_lod.rs:297`, `byroredux/src/cell_loader/placement_lod.rs:395` (deferred consumers)
- **Status**: NEW (was Existing: #1731 in AUDIT_LEGACY-COMPAT_2026-07-02 / LC0702-01; #1731 **closed** at parse scope by `175ebf2c`, leaving the consumer untracked)
- **Description**: The "Visible-When-Distant" / "Has Distant LOD" base-record-header flag (`0x00010000`) is the runtime signal the real engine reads to **cull the full model** once its quad's `.bto` / `_far.nif` LOD proxy is active. As of `175ebf2c` the flag is now parsed and exposed (`RecordHeader::is_visible_when_distant`, `FLAG_VISIBLE_WHEN_DISTANT` in reader.rs) and pinned by three assertions — but it has **zero production consumers**. The object-LOD (`.bto`) and placement-LOD (`_far.nif`) spawn paths still avoid the full-mesh + LOD-mesh z-fight conservatively by loading distant geometry *only outside the full-detail ring* — a coarser rule than the flag gives. #1731's title scoped it to "flag not parsed"; closing it removed the tracker for the remaining consumer work, and no follow-up issue was filed (grep of issues.json for `vwd` / `visible when distant` / `distant lod` / `1731` → 0 open hits).
- **Evidence**: `grep -rn is_visible_when_distant` across the whole tree resolves **only** to `crates/plugin/src/esm/reader.rs` (definition + 5 test-assertion lines) — no spawn/streaming caller. `object_lod.rs:297` — "The full-model VWD cull is deferred; quads load only outside the full-detail ring." `placement_lod.rs:395` — "VWD object in a `.lod` renders its full mesh at distance." `exal.md` §5.2 documents the culling rule as the runtime signal.
- **Impact**: A full REFR sitting right at the full-detail/LOD boundary can render alongside its LOD proxy → z-fight / doubled draw at the ring seam. Blast radius is the transition band only; the conservative ring rule prevents it in the common case, so runtime impact stays MEDIUM. The tracking gap is the more durable problem — the deferred consumer risks being re-derived from scratch or forgotten, exactly the stale-premise class `feedback_audit_findings.md` warns about (mirrors the LC0702-05 → #1849 tracking-gap remedy).
- **Related**: #1731 (CLOSED, parse scope); #1849 (OPEN, sibling untracked→tracked LOD gap NAM3/NAM4); EXAL §5.2 / §5.4; SKILL Dimension 5 (which already notes "wiring it into the object-LOD spawn path … is the remaining gap, not the parse itself").
- **Suggested Fix**: File a low-priority tracking issue ("VWD full-model cull consumer — flag parsed under #1731 but unconsumed; suppress full REFR beyond full-detail radius when its quad's LOD is active"), mirroring #1849's shape, so the deferred consumer has a tracker. No code change required to close the audit finding — the parse is correct and the conservative ring rule is a valid interim.

---

## Still-open tracked gaps re-verified this pass (Existing — do not re-file)

Spot-checked against live code; each remains accurate and tracked:

- **#1849** (LC0702-05) — WRLD NAM3/NAM4 LOD-water + OFST cell-offset table skipped. OPEN, correct. The tracking remedy recommended in the 2026-07-02 report was actioned.
- **#1718** (FNV-D7-01) — ragdoll body/constraint silent drop on bone-name miss. **CLOSED** at telemetry scope by `ffe9a816`: `template_from_imported` now collects `dropped_bones` / `dropped_constraint_bones` and emits `log::warn!` (ragdoll.rs:110-146). The underlying drop still occurs by design; the bone lookup stays **case-sensitive** (`skel_map.get(&b.bone_name)`), which correctly matches Gamebryo `NiFixedString` exact-match interning — a case-normalising compare would *diverge* from the source engine, so it is **not** a finding.
- **#1850 / #1851 / #1852** (FNV-D7-02/03/04) — bhkBreakableConstraint edge drop, unpinned body/joint counts, ragdoll-writeback scale. OPEN, PHYSAL-slice items owned by `/audit-fnv`/`/audit-physal`.
- **#1659** (SKY-D3-03) — BSDismemberSkinInstance body-part flags parsed/discarded. OPEN, NIFAL parked passthrough (no dismemberment consumer). Correct per no-fabrication rule.
- **#1856** (FO3-D1-01) — WaterShaderProperty flags dead-end at MaterialInfo. OPEN, NIFAL passthrough.

---

## Documented limitations re-confirmed (NOT findings — do not re-file)

Per the SKILL leak-inventory cross-check, still accurate:

- **FO4/FO76/Starfield ragdolls** — blocked on `BhkNPCollisionObject → BhkSystemBinary` blob decoder (PHYSAL §5).
- **`BhkPCollisionObject` phantoms** — parked pending a `TriggerVolume` ECS path (PHYSAL §5).
- **NIFAL parked passthroughs** — `bs_value_node`, `bs_ordered_node`, `tree_bones`, `range_kind`, `bs_lod_cutoffs`, `lod_group`, `bs_sub_index`, `NiSwitchNode`/`NiTextureEffect` (content-absent). Each blocked on a not-yet-built consumer (NIFAL §Nodes/§Passthroughs).
- **Animation parked channels** — per-light ambient + morph-weight channels (nifal.md §Animation). No visible-content loss; no consumer.
- **NiFogProperty** — intentionally not dispatched (#1224); 1 vanilla FO3 block; fog reads cell-scope `CellLighting` (NIFAL §Material).
- **Emissive scale** — three `EmissiveSource` variants share ~1.0 scale; no normalization is correct (NIFAL §4).
- **Sun latitude** — no authored CLMT/WRLD latitude field exists; `SUN_SOUTH_TILT` is engine-defined, not a parse gap (#1019 premise false; EXAL §9 Q1).

---

## Summary

- **Total findings**: 1
- **CRITICAL**: 0
- **HIGH**: 0
- **MEDIUM**: 1 (LC0703-01 — VWD full-model cull consumer untracked, NEW)
- **LOW**: 0

The compat surface remains in strong shape. All four single-producer canonical
boundaries and the coordinate single-source verify clean, and the renderer carries
zero per-game branches. The sole NEW finding is a **tracking** gap, not a code
defect: #1731 closed after landing the VWD flag *parse*, but the runtime
full-model-cull *consumer* was deferred and no follow-up issue tracks it. The
recommended remedy is a low-priority tracking issue mirroring #1849; the parse is
correct and the conservative full-detail-ring rule is a valid interim, so there is
no urgent code change.

---

*Next step:* `/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-07-03.md`
(LC0703-01 is the only NEW item — a tracking issue for the deferred VWD consumer.)
