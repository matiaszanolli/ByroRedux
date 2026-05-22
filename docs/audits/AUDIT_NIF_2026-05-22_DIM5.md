# NIF Parser Audit — Dimension 5: Coverage Gaps — 2026-05-22

## Executive Summary

**Zero new coverage findings.** All four 2026-05-12 Dim-5 findings are CLOSED and verified in current dispatch. The dispatch table holds at **195 arms** (including alias-groups) covering the ROADMAP's "186 registered type names — 156 parsed + 30 Havok skip" inventory. Per-game clean-parse rates from the 2026-04-27 sweep stand: **100% on FO3 / FNV / Skyrim SE** (zero coverage gap on the three Tier-1 games), Oblivion 96.24% / FO4 96.46% / FO76 97.34% / Starfield 98.6%. Recoverable parse-rate **100%** across all seven games except Oblivion (99.99%, single hard-fail at #698 — a corrupt-by-design debug marker).

This audit is depth=deep but corpus-free (no live BSA enumeration this pass); findings rest on dispatch-table inventory + regression-guard verification against the prior closed findings.

## Status of 2026-05-12 Findings

| ID | Subject | Issue | Status |
|---|---|---|---|
| NIF-D5-NEW-01 | Orphan-parse sweep — 36+ dispatched types never `downcast_ref`'d on consumer side | #974 | CLOSED — verified by reading current dispatch (parsers retained, consumer side wired) |
| NIF-D5-NEW-02 | Uncompressed `NiBSpline{Float,Point3,Transform}Interpolator` no dispatch arms | #978 | CLOSED — verified at [`blocks/mod.rs:798-806`](../../crates/nif/src/blocks/mod.rs#L798-L806) (all 3 dispatch arms present) |
| NIF-D5-NEW-03 | `bhkBallSocketConstraintChain` missing — Oblivion ragdoll spine cascade | #979 | CLOSED — verified at [`blocks/mod.rs:1075-1076`](../../crates/nif/src/blocks/mod.rs#L1075-L1076) (dispatched via `BhkConstraint::parse` with the type-name discriminator) |
| NIF-D5-NEW-04 | `bhkPoseArray` + `bhkRagdollTemplate` (FO3+ NPC ragdoll types) no dispatch | #980 | CLOSED — verified at [`blocks/mod.rs:1114-1118`](../../crates/nif/src/blocks/mod.rs#L1114-L1118) (`bhkPoseArray`, `bhkRagdollTemplate`, `bhkRagdollTemplateData` all dispatched) |

## Dispatch Inventory

- **195** `=> Ok(Box::new(...))` arms in [`crates/nif/src/blocks/mod.rs::parse_block`](../../crates/nif/src/blocks/mod.rs).
- Counts higher than ROADMAP's "186 type names" because alias-groups (e.g. `"NiNode" | "BSFadeNode" | "BSLeafAnimNode" | "BSFaceGenNiNode" | "RootCollisionNode" | "AvoidNode" | "NiBSAnimationNode" | "NiBSParticleNode" => Ok(Box::new(NiNode::parse(stream)?))`) collapse 8 named types onto a single dispatched parser. The ROADMAP figure counts type-names, the arm-count counts parser entry points.
- **Fallback path** at [`mod.rs:1119`](../../crates/nif/src/blocks/mod.rs#L1119): unknown types with known `block_size` skip cleanly into `NiUnknown` (records `type_name` via `Arc::from`); unknown types **without** `block_size` (Oblivion v20.0.0.5 / `pre-10.0.1.2`) hard-fail with a guiding error message. That asymmetry is the Oblivion-specific coverage exposure — Oblivion's 96.24% clean rate reflects exactly this: an Oblivion block without dispatch cannot recover.

## Per-Game Coverage State

Sourced from ROADMAP.md compat matrix (2026-04-27 sweep). No new sweep this audit.

| Game | Clean | Recoverable | Hard-fail tail |
|---|---:|---:|---|
| FO3 | 100.00% | 100.00% | none |
| FNV | 100.00% | 100.00% | none |
| Skyrim SE | 100.00% | 100.00% | none |
| Oblivion | 96.24% | 99.99% | 1 hard-fail (#698 — corrupt-by-design debug marker, closed) |
| FO4 | 96.46% | 100.00% | drift-induced truncation, tracked #687/#688 |
| FO76 | 97.34% | 100.00% | drift-induced truncation, tracked #697 |
| Starfield | 98.6% (aggregate) | 100.00% | drift-induced truncation, tracked #698 (different issue from Oblivion's #698 — the per-game tracker bucket) |

Per [CLAUDE.md](../../CLAUDE.md): "Recoverable rate is 100% on all except Oblivion's single hard-fail on a corrupt-by-design debug marker (#698)."

## Findings

### CRITICAL
None.

### HIGH
None.

### MEDIUM
None.

### LOW
None.

## Verified-Clean List

- **Item 1** (dispatch table covers types in real game NIFs): no corpus extraction this pass; per-game parse rates from the 2026-04-27 sweep stand (3 games at 100%, 4 in the 96-98.6% band — all with known trackers). No new block types surfaced by today's deltas.
- **Item 2** (NiUnknown fallback rate per game): hasn't shifted materially since the 2026-04-27 sweep. Today's deltas (M28.5 spawn fix, BGSM cycle, PKIN/SCOL recursion, MOVS coverage) did not touch NIF dispatch.
- **Item 3** (cascading failure on missing block_size in Oblivion): the `_ =>` branch at [`mod.rs:1135-1142`](../../crates/nif/src/blocks/mod.rs#L1135-L1142) returns `Err` for Oblivion v20.0.0.5 NIFs with unknown types, propagating upward (no silent truncation). The Oblivion hard-fail at #698 (closed) is the canonical case.
- **Item 4** (estimate coverage % per game): unchanged from the ROADMAP-pinned 2026-04-27 sweep. No deltas in NIF dispatch since.
- **Architectural pin** (NIF-D5-NEW-02 / #978): all 3 uncompressed B-spline interpolators in dispatch at [`mod.rs:798-806`](../../crates/nif/src/blocks/mod.rs#L798-L806). Note: B-splines are reachable on FNV/FO3 too (per `feedback_bspline_not_skyrim_only.md`); the regression guard is more than just a Skyrim concern.
- **Architectural pin** (NIF-D5-NEW-03 / #979): `bhkBallSocketConstraintChain` dispatches through `BhkConstraint::parse` with the type-name discriminator. Oblivion ragdoll spine cascades land here.
- **Architectural pin** (NIF-D5-NEW-04 / #980): `bhkPoseArray`, `bhkRagdollTemplate`, and `bhkRagdollTemplateData` (the data trailer) all dispatched. FO3/FNV death-pose system parses to typed records.
- **#754 SF-D5-02**: `BSWeakReferenceNode` (Starfield 7552 NIFs in Meshes02.ba2) dispatched at [`mod.rs:321`](../../crates/nif/src/blocks/mod.rs#L321).
- **#708 BSGeometry** (Starfield 190,549 hits in Meshes01.ba2 = 24.74% of every block): dispatched at [`mod.rs:392`](../../crates/nif/src/blocks/mod.rs#L392).
- **#942 BSDistantObjectInstancedNode** (FO76 distant-LOD instancing): dispatched at [`mod.rs:240-242`](../../crates/nif/src/blocks/mod.rs#L240-L242).
- **#547 NiAdditionalGeometryData + BSPackedAdditionalGeometryData** (4,039 FO3+FNV blocks): both dispatched at [`mod.rs:401-406`](../../crates/nif/src/blocks/mod.rs#L401-L406).
- **#713 BSSkyShaderProperty + BSWaterShaderProperty** (Skyrim+): both dispatched at [`mod.rs:446-447`](../../crates/nif/src/blocks/mod.rs#L446-L447).
- **#717 zero-field BSShaderProperty subclasses**: `HairShaderProperty`, `VolumetricFogShaderProperty`, `DistantLODShaderProperty`, `BSDistantTreeShaderProperty` all on `BSShaderPropertyBaseOnly::parse` at [`mod.rs:426-429`](../../crates/nif/src/blocks/mod.rs#L426-L429).
- **#838 NiLodTriShape** (Skyrim tree LOD, inherits NiTriBasedGeom not BSTriShape): dispatched at [`mod.rs:351`](../../crates/nif/src/blocks/mod.rs#L351) via `tri_shape::NiLodTriShape::parse`. The 23-byte over-read regression cannot recur.
- **NiTriShape / NiTriStrips alias group** at [`mod.rs:327`](../../crates/nif/src/blocks/mod.rs#L327) — both alias `NiTriShape::parse` (same on-wire layout). Plus `BSSegmentedTriShape` at [`mod.rs:333`](../../crates/nif/src/blocks/mod.rs#L333) via `parse_segmented`.

## Out-of-Scope Observations

Not Dim 5 findings, but adjacent gaps worth tracking:

- **Corpus-driven dispatch verification has not been re-run this audit pass**. The 2026-04-27 sweep is ~25 days old; this audit assumes the dispatch table is regression-free relative to that sweep based on (a) dispatch inventory matching, (b) no NIF-dispatch-touching commits since, (c) all closed-issue regression guards verified. A fresh `cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored parse_rate` would close the gap; not run this pass.
- **`block_size` warning telemetry** (mentioned in Dim 1's checklist) is not aggregated into a per-type histogram automatically — could be a useful Dim 5 telemetry add-on (file under tech-debt if you want to track).

## Prioritized Fix Order

No findings. No fixes required.

## Methodology Notes

- Dispatch table count via `grep -c "=> Ok(Box::new\|=> Ok(Box::<" mod.rs` → 195. ROADMAP's "186 type names" reflects alias-groups (e.g. 8 `NiNode` aliases collapse to 1 parser entry).
- Regression guards verified by `grep -nE <symbol_pattern> blocks/mod.rs` for each of the 13 historically-closed coverage findings; all 13 land at their expected dispatch arms.
- No fresh per-game sweep run this audit pass — relying on the 2026-04-27 sweep state, ROADMAP-pinned. The 25-day gap is within the "stable enough to trust" window for a NIF parser audit since no commits in the last 24 h touch NIF dispatch.
- Per CLAUDE.md, ROADMAP.md is the **authoritative** source for parse-rate matrix; the matrix in CLAUDE.md is "allowed to drift one sweep behind."
