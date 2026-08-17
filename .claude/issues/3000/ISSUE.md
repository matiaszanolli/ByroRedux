# RT-2026-08-16-01: p2-melee-core.sh passes while its fixture's player has no floor — asserts physics_synced, never grounded

**Issue**: #3000
**Severity**: HIGH
**Dimension**: Playable-slice gate semantics
**Labels**: `high,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_RUNTIME_2026-08-16.md` (Dimension — Playable-slice gate semantics).

**Location**: `docs/smoke-tests/p2-melee-core.sh`:144-160 · `byroredux/src/commands/view.rs`:186-245 · `byroredux/src/systems/character.rs`:707-726

## Description

`p2-melee-core.sh` is the gate that certifies the P2 melee core. For each of its seven swings it asserts `physics_synced=true` — that the character body was positioned — but **never asserts `grounded`**. The fixture's player has no floor beneath it, and the gate passes anyway because nothing it checks can observe that.

## Evidence

```bash
# docs/smoke-tests/p2-melee-core.sh:146
grep -Fq "physics_synced=true" "$command_log" \
    || fail "swing $hit did not position the real character body"
```

Re-verified 2026-08-17: the loop greps `physics_synced=true`, `queued Attack through the R binding`, `hits=$hit`, `damage=8.0`, `health_after=…` and `cooldown=0.000`. **No `grounded` assertion anywhere in the script.**

## Impact

The gate certifies "the character body is where combat thinks it is" while remaining blind to whether that body is standing on anything. A regression that leaves the player falling — or spawned in void — keeps this gate green for as long as the swing arithmetic still resolves.

This is the primary runtime gate for the project's active execution focus, so a blind spot here is load-bearing.

## Suggested Fix

Assert `grounded=true` alongside `physics_synced=true` in the per-swing loop, and fix the fixture so the player actually has a floor. If the fixture is deliberately floorless, say so in `docs/engine/p2-combat-fixture.md` and assert the intended state explicitly rather than omitting the check.

## Related

- RT-2026-08-16-03 (#3002) — the walkable-spawn gate certifies a different column than the one the character spawns in
- RT-2026-08-16-09 (#3008) — the same gate's other missing assertions

## Completeness Checks
- [ ] **SIBLING**: `p0-door-interaction.sh` and `p1-character-traversal.sh` checked for the same missing groundedness assertion
- [ ] **FIXTURE**: The fixture gives the player a real floor, or the floorless state is documented and asserted deliberately
- [ ] **GATE-TRACKS-CONTRACT**: The assertion pins the invariant, not the current observed value
- [ ] **TESTS**: The gate fails if the player is un-grounded

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3000 --json state` when live state is needed.*
