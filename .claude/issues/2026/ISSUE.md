# SCR-D7-NEW2-01: A SCOL/PKIN outer REFR's own VMAD is replicated onto every expanded synthetic child instead of attaching once

**Labels**: medium, import-pipeline, bug

**Severity**: MEDIUM
**Dimension**: Engine Attach Path & Trigger-Volume Wiring
**Untrusted-Input**: Yes (reachable via any plugin placing a VMAD-scripted REFR whose base form is SCOL/PKIN with no cached merged model)
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-07-16.md`

## Location
`byroredux/src/cell_loader/references/mod.rs:365-373` (the `synth_refs` expansion loop), consumed at `:463-469`, `:479-487`, `:784-792` — all pass the same `placed_ref.script_instance.as_ref()` (the outer REFR's own VMAD) for every synthetic child.

## Description
`expand_pkin_placements`/`expand_scol_placements` (`byroredux/src/cell_loader/refr.rs:357-450`) fan one placed REFR out into N synthetic child placements. The REFR's own VMAD (a per-instance Skyrim+ Papyrus attachment) is a property of the single outer REFR, but the expansion loop threads it unchanged into `attach_script_for_refr` for every synthetic child. Each child is a distinct ECS entity, so a VMAD-scripted SCOL/PKIN REFR's canonical behavior (including the `OnCellLoadEvent` that follows a successful attach) is instantiated N times instead of once. This mirrors the deliberate, correct sharing already special-cased for texture overlays (#584 — correct because a visual re-skin is identical per piece), but VMAD attachment is behavioral, not visual, and has no equivalent rationale.

Verified current: `byroredux/src/cell_loader/references/mod.rs` still reads `placed_ref.script_instance.as_ref()` unchanged at all three cited call sites for every synthetic child produced by the `synth_refs` expansion loop.

## Evidence
`refr.rs:490-505` shows SCOL parts fanning from one `base_form_id`; `mod.rs:469/484/789` all read the outer `PlacedRef.script_instance`, never re-scoped per synthetic child (unlike `child_form_id`, which correctly varies). No test exercises a VMAD-carrying SCOL/PKIN REFR.

## Impact
N independent copies of the same recognized behavior spawn per decorative piece rather than once per logical object. Harmless for an idempotent effect (`SetStage` to the same target N times is a no-op after the first) but would fire once per piece for a non-idempotent side effect (item grant, spawn, sound), and the cell-load init hook fires N times instead of once. Narrow trigger — requires SCOL/PKIN with no merged model plus REFR-level VMAD, a combination not observed in vanilla content — hence MEDIUM.

## Related
Distinct from #1737 (REFR-vs-base-record VMAD source precedence); distinct from the deliberate, correct `refr_overlay` sharing (#584) this finding contrasts against.

## Suggested Fix
Attach the outer REFR's own `script_instance` only to the first synthetic child (or a dedicated non-rendering placement-root entity), passing `None` for the remaining N-1 children — mirroring how `door_pos` already special-cases "first of N" elsewhere in the same loop. Alternatively gate REFR-own-VMAD attach on `synth_refs.len() == 1` and trace-log when a multi-piece expansion carries a VMAD.

## Completeness Checks
- [ ] **SIBLING**: Confirm the deliberate `refr_overlay` (#584) sharing pattern is NOT similarly mis-applied elsewhere for behavioral (non-visual) properties
- [ ] **TESTS**: A regression test pins this specific fix (VMAD-carrying SCOL/PKIN REFR fixture)
