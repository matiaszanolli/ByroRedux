# RT-2026-08-16-03: walkable-spawn gate certifies the camera column while the character spawns at the door column

**Issue**: #3002
**Severity**: HIGH
**Dimension**: Playable-slice gate semantics
**Labels**: `high,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_RUNTIME_2026-08-16.md` (Dimension — Playable-slice gate semantics).

**Location**: `byroredux/src/scene.rs`:883-903, :1122-1165

## Description

The walkable-spawn ground probe runs `probe_spawn_ground(world, cam_pos, cc_for_probe)` — it casts down from the **camera** position. The character is spawned at the **door** column. The gate therefore certifies a column the character does not occupy, and its notion of "floor" disagrees with the one the character controller uses.

## Evidence

```rust
// byroredux/src/scene.rs:901
let probe = probe_spawn_ground(world, cam_pos, cc_for_probe);
```

`cam_pos` is the camera column; the interior spawn path places the character at the first door's own placement (see [Interior Spawn Point Fix]). The probe also builds its own `CharacterController::HUMAN` for the cast rather than reading the controller the character will actually use, so the two can disagree on capsule dimensions and ground tolerance.

Re-verified 2026-08-17.

## Impact

A cell can pass the walkable-spawn gate while the character lands somewhere with no floor — which is exactly the state RT-2026-08-16-01 (#3000) shows the P2 gate cannot detect. The two findings compound: one gate probes the wrong column, the other never checks groundedness at all.

## Suggested Fix

Probe the column the character will actually spawn in, and use the character's own `CharacterController` rather than a locally-constructed `HUMAN` default, so the probe's "floor" and the controller's "floor" are the same predicate.

## Related

- #3000 (RT-2026-08-16-01 — the P2 gate that would have caught the consequence)
- [Interior Spawn Point Fix] — the door-placement spawn rule this probe predates

## Completeness Checks
- [ ] **SAME-COLUMN**: The probe and the spawn use one position, derived once
- [ ] **SAME-CONTROLLER**: The probe uses the character's real controller, not a fresh `HUMAN`
- [ ] **SIBLING**: The exterior spawn path checked for the same camera-vs-character divergence
- [ ] **TESTS**: A regression test pins probe column == spawn column

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3002 --json state` when live state is needed.*
