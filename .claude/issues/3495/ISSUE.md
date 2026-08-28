# Issue #3495: PHYS-D7-2026-08-27b-04: byroredux/src/commands/physics.rs missing from the /audit-physics scope list and _audit-common.md Commands row

Labels: low, tech-debt, documentation, doc-rot
Filed: 2026-08-27 (published 2026-08-28)

---

**Source**: `docs/audits/AUDIT_PHYSICS_2026-08-27b.md` — PHYS-D7-2026-08-27b-04
**Severity**: LOW · **Dimension**: 7 — Queries, Diagnostics & Cost

## Location
- `.claude/commands/audit-physics/SKILL.md` § Scope ("Engine-side (Dimensions 4 + 6)" list)
- `.claude/commands/_audit-common.md` § Project Layout, the `Commands:` row
- The un-listed file `byroredux/src/commands/physics.rs` (200 LOC)

## Trigger Conditions
Any `/audit-physics` run that follows the skill's scope list literally.

## Description
`#2876` added `phys.census` and `phys.stats` in a new `byroredux/src/commands/physics.rs`, promoting `PhysicsWorld`'s whole query surface (`colliders_near_xz`, `static_colliders_aabb`, `cast_capsule_down*`, `body_count`, `awake_counts`) to the live console. Dimension 7's checklist explicitly asks whether `dump_spawn_collider_census` "is reachable from `byro-dbg`" — and the file that makes it reachable is named neither in the skill's scope list (which still names only `commands/scene.rs` and `commands/water.rs`) nor in `_audit-common.md`'s per-domain `Commands:` row (which enumerates `world_info`, `assets`, `view`, `scene`, `actor_value`, `condition`, `time`, `water`, `quest`, `env_health`, `shared` — eleven of twelve).

## Evidence
```
$ ls byroredux/src/commands/physics.rs
byroredux/src/commands/physics.rs
$ grep -n "commands/physics" .claude/commands/_audit-common.md .claude/commands/audit-physics/SKILL.md
(no match in either file)
$ grep -n "commands/" .claude/commands/audit-physics/SKILL.md
47:`byroredux/src/commands/scene.rs` (the `ragdoll` console command),
48:`byroredux/src/commands/water.rs`, and the parse side
```
The commands are real and registered: `byroredux/src/commands_tests.rs:986` asserts `["phys.census", "phys.stats"]` are in the registry and `:1034` pins their argument handling. Both greps re-run at publish time on HEAD — still no match.

## Impact
Audit-scope rot — the class `_audit-common.md`'s Path-Reference Convention exists to prevent. A physics-diagnostics audit that follows the scope list examines the census *producer* and never its console *consumer*. Concretely, the `-27b` pass only found by going off-list that `phys.census` sweeps `CharacterController::HUMAN` rather than the live player's controller (`byroredux/src/commands/physics.rs:117-119`) — harmless today only because the spawn rungs also use `HUMAN`, and silently wrong the moment they diverge.

## Related
`#2876`; the "Un-owned subsystems" coverage table in `_audit-common.md`.

## Suggested Fix
Add `byroredux/src/commands/physics.rs` to the `/audit-physics` Dimension 7 entry-point list and to `_audit-common.md`'s `Commands:` row, then run `.claude/commands/_audit-validate.sh`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
