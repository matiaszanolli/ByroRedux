# #2390 — ECS-D7-2026-08-07-03: `/audit-ecs` SKILL.md's seat-claim checklist item is stale — describes the pre-#2147 wholesale clear

- **Severity**: LOW
- **Domain**: ecs, documentation
- **Audit**: `docs/audits/AUDIT_ECS_2026-08-07.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2390


- **Severity**: LOW
- **Dimension**: 7 — Component Lifecycles (audit-checklist doc rot)
- **Location**: `.claude/commands/audit-ecs/SKILL.md:251-256`
- **Status**: NEW

**Description**

The checklist still says seat claims are "wholesale-cleared on every cell load ... since entity ids reset on unload" and asks the auditor to verify the multi-cell double-claim risk. That behaviour was replaced by `prune_seat_reservations` under #2147, and the production comment at `references/mod.rs:311-316` explicitly refutes the "entity ids reset on unload" premise the skill repeats: "They don't — `World::despawn` documents that IDs are never reclaimed (#372) and `next_entity` only grows."

**Evidence**: `references/mod.rs:317` calls `prune_seat_reservations(world)`, whose body is a `retain`, not a `clear`. `grep -rn "clear_seat_reservations"` → no hits.

**Impact**

Sends every future ECS auditor to re-verify a fixed bug and to accept a false premise about entity-ID recycling.

**Related**: #2147; ECS-D7-2026-08-07-02 (the genuine remaining gap — file that one alongside this).

**Suggested Fix**: Rewrite the bullet to describe `prune_seat_reservations`'s furniture-liveness retain and point the auditor at the claimant-liveness half instead.

## Completeness Checks
- [ ] **DOC**: Rewrite `.claude/commands/audit-ecs/SKILL.md:251-256` to describe `prune_seat_reservations`'s actual furniture-liveness retain, and remove the "entity ids reset on unload" premise
- [ ] **SIBLING**: Scan the rest of the skill's M42 checklist bullets for similarly stale pre-#2147 assumptions

---
Filed from `docs/audits/AUDIT_ECS_2026-08-07.md` via `/audit-publish`.
