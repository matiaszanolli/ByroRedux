# FO3-D4-01: PLAYER_BASE_FORM_ID = 0x14 matches no NPC_ record in any game

**Issue**: #3099
**Severity**: HIGH
**Labels**: `high,gameplay,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_FO3_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_FO3_2026-08-16.md` (Dimension 4 — player bring-up).

**Location**: `byroredux/src/inventory.rs`:17 (constant), :119-121 (`build_player_template` early return), :242-252 (`attach_to_player`), :476 (the test that cements it)

## Description

`PLAYER_BASE_FORM_ID = 0x14` **matches no `NPC_` record in any game** — the player spawns with no inventory, no armor and no weapon.

`0x00000014` is the player **reference** (the `ACHR`/`REFR` instance). The player's **base `NPC_` record** is a different FormID. Looking `0x14` up in `index.npcs` therefore always misses.

## Evidence

```rust
// byroredux/src/inventory.rs:17
const PLAYER_BASE_FORM_ID: u32 = 0x0000_0014;

// :119-121 — the miss, silently defaulted
fn build_player_template(index: &EsmIndex) -> PlayerInventoryTemplate {
    let Some(player) = index.npcs.get(&PLAYER_BASE_FORM_ID) else {
        return PlayerInventoryTemplate::default();
```

Re-verified 2026-08-17. The `else` arm returns an empty template with no diagnostic.

Note `.claude/commands/_audit-common.md` repeats the same claim — *"seeds the player from base `NPC_` 0x00000014"* — so the error is mirrored in the audit infrastructure.

## Impact

The player has no starting inventory, armor or weapon in **any** game. `prefer_weapon` has nothing to choose from, so `EquippedWeapon` is never attached, so `attack_damage` falls to `UNARMED_DAMAGE = 8.0`.

**This is why `p2-melee-core.sh` asserts `damage=8.0`** (#3008): the gate encodes the consequence of this bug as its pass condition. Fixing this turns that gate RED for the right reason.

## Suggested Fix

Resolve the player's base `NPC_` FormID correctly per game (it is not the reference id), and make the lookup miss **loud** — a silent default template is what hid this.

Correct `_audit-common.md`'s matching claim in the same pass.

## Related

- **#3008 (RT-09 — the gate that pins `damage=8.0`, i.e. this bug's symptom)**
- #3032 (ECS-04 — the inventory cannot equip a weapon even when one exists)
- #2992 (FO4 weapon damage decodes to zero — the third independent reason the player deals no weapon damage)

## Completeness Checks
- [ ] **PER-GAME**: The base `NPC_` id is resolved correctly for each target game, not hardcoded to one value
- [ ] **NOT-SILENT**: A failed player-template lookup logs rather than defaulting quietly
- [ ] **SKILL-DOC**: `_audit-common.md`'s "base `NPC_` 0x00000014" claim corrected
- [ ] **GATE**: #3008 updated so it does not assert the unarmed fallback as correct
- [ ] **TESTS**: A regression test asserts a non-empty player template on real data

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3099 --json state` when live state is needed.*
