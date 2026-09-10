# #2532: NIFAL-D9-04: The new canonical-tier completeness harness covers 1 of ~5 declared translate boundaries

Labels: bug, nif-parser, medium, tech-debt, nifal

---

**Severity**: MEDIUM
**Dimension**: Completeness · **Tier Violated**: (harness gap — no production tier violated; same classification `#2213`/`#2214` used pre-fix)
**Game Affected**: all seven (harness-coverage gap, not a per-game data bug)
**Location**: `byroredux/src/material_translate.rs:571-574` (the scoping comment); no equivalent kitchen-sink module exists for `crates/nif/src/import/collision/shape.rs::resolve_shape_inner`, `byroredux/src/anim_convert.rs::convert_nif_clip` (+ `byroredux/src/asset_provider/animation.rs::convert_hkx_clip`), `byroredux/src/systems/particle.rs::apply_emitter_overlays`, or `crates/nif/src/import/walk/mod.rs`'s `LightKind` resolution
**Status**: NEW (successor to `#2214`'s residual scope; `#2214` itself is now closed as Material-scoped, verified genuinely functional by an independent revert-and-fail test)

## Description
The six translate-boundary bugs a prior sweep found and cited as evidence the completeness harness was needed were NIFAL-D6-01, D6-02, D3-01, D4-02, D6-03, D6-04 — four in Collision, one in Lights, one in Nodes. **Zero were in Material.** The kitchen-sink harness added by `#2214` (`byroredux/src/material_translate.rs:547-798`, `mod canonical_completeness_harness`) is scoped to Material only; its own doc comment says "collision/animation have no `translate_*` boundary yet to extend it to" — but per `docs/engine/nifal.md` itself, both categories *do* have declared, named, "converged"/"audited" boundaries. What's missing for those categories is not the boundary but a kitchen-sink canonical-output completeness test of the kind `#2214` just wrote for Material; the harness's scoping comment understates what already exists.

## Evidence
Confirmed directly: `grep -rln "kitchen_sink" crates/nif/src byroredux/src` returns only `byroredux/src/material_translate.rs`. The four fixed Collision bugs and the Lights bug were all caught by manual code tracing in prior sweeps — no automated harness existed for those categories then, and none exists now. Collision's own dimension independently caught and fixed two further boundary bugs this delta (`#2285`/NIFAL-D6-07, `#2298` triple-duplicated destrip logic) — again by manual trace, supporting evidence for this finding.

## Impact
The completeness *signal* is real for one of ~9 NIFAL categories. "Dimension 9 passes" cannot be read as "the translation layer's output is regression-tested" beyond Material — the other categories still depend entirely on manual audit sweeps catching drift.

## Related
Successor/residual scope of closed `#2214` (NIFAL-D9-02).

## Suggested Fix
Extend the `canonical_completeness_harness` pattern in priority order: Collision (`resolve_shape_inner` — highest historical bug count, 4/6), Lights (`LightKind` resolution — 1/6), Animation (`convert_nif_clip`/`convert_hkx_clip`). Also correct the scoping comment at `material_translate.rs:571-574` regardless of extension timing — it currently reads as though Collision/Animation have no boundary at all, contradicting `nifal.md`'s own "converged"/"audited" verdicts.

## Completeness Checks
- [ ] **TESTS**: Extend the kitchen-sink harness pattern to at least the Collision boundary (`resolve_shape_inner`) as the first extension
- [ ] **CANONICAL-BOUNDARY**: New harness modules mirror the Material harness's revert-and-fail self-verification approach




# #2625: SF-D6-04: opaque-tail capture disables drift telemetry that would have surfaced SF-D6-01 for free

Labels: bug, nif-parser, medium, legacy-compat, game:starfield

---

**Severity**: MEDIUM
**Dimension**: 6 (NIF Shader Blocks, BSVER 155+)
**Location**: `crates/nif/src/blocks/shader.rs:760-778` (`read_starfield_tail`), `crates/nif/src/lib.rs:460-508` (drift accounting)
**Status**: NEW

## Description
`read_starfield_tail` consumes `block_size − consumed` *before* `parse_nif`
compares consumed against `block_size`, so any Starfield shader-block
under-read is converted into tail bytes and never reaches
`drift_histogram`. Measured: shader-block drift is `{}` (empty) on all four
archives while tail lengths were simultaneously bimodal `{38: 1868, 42:
11}` — exactly the signal a drift histogram exists to raise, invisible to
`nif_stats --drift-histogram`.

## Evidence
`crates/nif/src/blocks/shader.rs:760-778` captures the tail before the
drift comparison in `crates/nif/src/lib.rs:460-508` ever runs.

## Impact
Blind spot on precisely the block types with the most Starfield churn;
one-directional (over-reads still surface via `saturating_sub` → empty
tail), but under-read is the failure mode these parsers actually exhibit —
this exact mechanism is why SF-D6-01 went undetected by existing drift
telemetry.

## Suggested Fix
Have `read_starfield_tail` also record captured length into a per-type
`opaque_tail_histogram` sibling of `drift_histogram`, surfaced on
`NifScene`.

## Related
SF-D6-01, SF-D6-02.

## Completeness Checks
- [ ] **TESTS**: A synthetic under-read fixture asserts `opaque_tail_histogram` flags the anomaly




# #2637: SF-D4-06: sf_smoke unresolved-REFR report conflates by-design exclusions with real gaps, overstates ~5x

Labels: bug, import-pipeline, low, legacy-compat, game:starfield, esm-plugin

---

**Severity**: LOW
**Dimension**: 4 (Starfield ESM Resolve-Rate Baseline)
**Location**: `byroredux/src/sf_smoke.rs:154-176`
**Status**: NEW

## Description
`sf_smoke`'s "unresolved" report conflates by-design exclusions with real
gaps, overstating the headline number ~5×. The tool's hint text ("parser
gap — schema diverged or record type missing") applies uniformly, but this
run's 2,461 unresolved `citycydoniamainlevel` REFRs decompose into ~140
(0.5%) real #1576 gap, 1,846 (6.6%) intentionally-unconsumed PDCL, and ~369
(1.3%) by-design-excluded audio markers.

## Evidence
Breakdown of the 2,461 unresolved REFRs on `citycydoniamainlevel` into the
three buckets above.

## Impact
Purely a diagnostic-tool clarity gap — but it's exactly the failure mode
the tool exists to catch: a real regression (PDCL doubling, or the BFCB gap
widening) would today hide inside the same undifferentiated bucket as two
already-understood causes.

## Suggested Fix
Thread the FourCC through the existing skip telemetry into a per-type
unresolved-REFR counter; separate known-tracked buckets from the residual
"unattributed" count in the report.

## Related
SF-D4-05, #1576, #1568.

## Completeness Checks
- [ ] **TESTS**: A fixture with a mix of the three bucket types asserts the report separates them correctly




# #2699: FO4-D1-02: The new "interactive XPRI retain" gate restores gameplay identity but not visual de-dup, contradicting the documented XPRI contract

Labels: bug, import-pipeline, medium, legacy-compat, game:fo4, esm-plugin

---

- **Severity**: MEDIUM
- **Dimension**: 1 — precombines / cell load
- **Location**: `byroredux/src/cell_loader/references/mod.rs:121-133`, `:420-434`
- **Status**: NEW (introduced `fd3f7080`, 2026-08-10 — after the 2026-08-07 audit)
- **Description**: `fd3f7080` narrowed the absorption skip to `STAT | SCOL` via `precombine_can_replace_record`; any other base record type (`FURN`, `CONT`, `ACTI`, `TERM`, `MSTT`, `DOOR`, …) listed in XPRI now falls through and is spawned **in full, geometry included**. No mechanism suppresses just the 3D. The XPRI contract documented one file over (`crates/plugin/src/esm/cell/walkers.rs:298-305`: *"their geometry is already baked into the `_oc.nif` files"*) and again at `byroredux/src/cell_loader/precombined.rs:6-9` directly contradicts the new comment's premise that dropping the REFR *"drops both their visuals and their gameplay identity"*. Both cannot be true.
- **Evidence**: the retain branch increments `job.absorbed_interactive_retained` and falls through to the ordinary spawn path — no `RenderLayer` change, no mesh-free ghost spawn, and no cross-check against what `spawn_precombined_meshes` actually emitted for the cell.
- **Impact**: Switchboard alone is cited in the new comment as carrying 141 such REFRs. If their geometry is in the bake — which XPRI asserts — that is 141 duplicated meshes in one cell, z-fighting and doubled BLAS entries on precisely the interactive props the player inspects closest. If it is *not* in the bake, the walker and loader doc comments are wrong and actively mislead future work on this path.
- **Related**: #1188, #2593, FO4-D1-01.
- **Suggested Fix**: Settle the contract from measured data (compare a Switchboard XPRI `FURN`/`MSTT` against the decoded CSG object set), then either retain the entity but strip the renderable — Bethesda's own "hide 3D, keep ref" model — or correct the comments if the bake genuinely excludes non-STAT records. Either way the two comments must stop contradicting each other.

---
**Source**: `docs/audits/AUDIT_FO4_2026-08-12.md` (finding `FO4-D1-02`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix




