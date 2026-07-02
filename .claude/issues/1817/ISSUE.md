# SCR-D6-NEW-02: Trigger volume's occupant_inside not seeded from initial containment — spurious enter-fire when player loads already inside

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1817
**Source report**: docs/audits/AUDIT_SCRIPTING_2026-07-02.md
**Labels**: medium, legacy-compat, bug

- **Severity**: MEDIUM
- **Dimension**: Engine Attach & Trigger Wiring (runtime consequence)
- **Untrusted-Input**: No
- **Location**: `byroredux/src/cell_loader/references.rs:1455-1461` (`trigger_volume_from_primitive`) + `crates/scripting/src/trigger.rs:114-120`
- **Status**: NEW

**Description**: `trigger_volume_from_primitive` hardcodes `occupant_inside: false` at spawn. `trigger_detection_system` fires `OnTriggerEnterEvent` on the `inside && !occupant_inside` edge. When the player begins a cell/save load *already standing inside* a trigger volume, frame-1 detection sees `inside == true` against the seeded `false` and fires a spurious enter — i.e. level-triggered-on-load rather than edge-triggered. Bethesda's `OnTriggerEnter` semantics fire only on an actual outside→inside crossing.

**Evidence**: spawn site sets `occupant_inside: false` unconditionally; the audit's Dim-6 seed contract ("a player loaded already inside a volume must NOT spuriously fire on frame 1 — `occupant_inside` seeded true") is unmet. Distinct from #1742 (which is about the *rotation frame* of the permuted half-extents).

**Impact**: A quest gated on `OnTriggerEnter` can advance the instant the player loads a save while inside the trigger box, even though they never crossed the boundary that frame — silent game-logic corruption on load. Realistic for autosaves taken inside a scripted trigger region.

**Related**: #1742 (trigger-box rotation frame), #1727 (drain, fixed).

**Suggested Fix**: Seed `occupant_inside` from the volume's containment of the player's initial world position at spawn (or run one silent "prime" pass of `trigger_detection_system` that updates `occupant_inside` without emitting markers before the first gameplay frame).

## Completeness Checks
- [ ] **SIBLING**: Same seeding gap checked for any other edge-triggered ECS marker spawned mid-load (e.g. other containment-based triggers)
- [ ] **TESTS**: A regression test pins this specific fix
