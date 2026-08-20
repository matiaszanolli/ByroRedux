# TD4-2026-08-20-02: audit-physics/SKILL.md instructs auditors to "confirm absence rather than reporting it" for code that shipped

**Issue**: #3199 — https://github.com/matiaszanolli/ByroRedux/issues/3199
**Severity**: MEDIUM
**Labels**: `medium,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-20.md`
**Filed**: 2026-08-20 · `/audit-publish` · verified against HEAD `bb0b92f2`

---

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-08-20.md` § TD4-2026-08-20-02 (Dimension 4 — Audit-Finding Rot).

**Severity**: MEDIUM · **Effort**: trivial
**Location**: `.claude/commands/audit-physics/SKILL.md:290-291` (the Dimension 6 checklist line) and `:60-63`; `.claude/commands/_audit-common.md:134` (the `docs/engine/watal.md` row of the Key Reference Docs table).

## This is a correctness defect in the audit tooling, not doc rot

An audit skill file that tells an auditor **what not to look for** is a failure mode no gate can catch. The path gate checks paths. The symbol advisory checks symbols. Neither can evaluate an *instruction*. The only thing standing between this line and two lost findings was one auditor's skepticism.

## The line

`.claude/commands/audit-physics/SKILL.md:290`, inside Dimension 6's checklist:

> `- Character swimming/drowning are **unbuilt** (WATAL open items). Confirm absence rather than reporting it.`

An auditor following it **skips the newest, least-reviewed code in the subsystem it is dispatched to audit**. Sibling text at `:60-63` and `_audit-common.md:134` repeats the "open items are character swimming/drowning" claim.

## The claim is false as of this delta

Character swimming and bounded drowning damage shipped in `c7561d74` (2026-08-19). Live at HEAD in `byroredux/src/systems/character.rs`:

```
953:const SWIM_HEIGHT_SCALE: f32 = 0.35;
956:fn swimlevel_reached(center_y, surface_y, half_span) -> bool
964:pub(crate) fn swim_vertical_velocity(...)
994:fn advance_breath(...)
1001:    const DROWNING_DAMAGE_PER_SECOND: f32 = 12.0;
1027:fn apply_player_drowning_damage(world, player, damage)
```

wired into `character_controller_system` at `:237`, `:239`, `:268` and `:478-483`, with inline tests at `:1363-1383`.

**Timeline** — the text was accurate when written and rotted underneath the code:

| When | What |
|---|---|
| 2026-08-13 | `0c129879` adds the "unbuilt / confirm absence" line to `audit-physics/SKILL.md` — **correct at the time** |
| 2026-08-19 | `c7561d74` ships the swim + drowning core |
| 2026-08-20 | `docs/engine/watal.md` refreshed and **correct** (`:22-23` "Character swimming and bounded drowning damage are live"; `:408`; `:617-618`). The two audit-infrastructure files were not |

## It nearly fired — this is not hypothetical

`docs/audits/AUDIT_PHYSICS_2026-08-20.md:81-88` opens a section titled *"WATAL spec drift worth recording (not a bug)"*:

> `docs/engine/watal.md`'s open-items list — and this audit skill's Dimension 6 instruction to *"confirm absence rather than reporting it"* — say character swimming/drowning are unbuilt. **They are built as of this delta** … **Two findings below are *in* that new code.**

The two findings the instruction would have suppressed:

- **#3119** (PHYS-D4-2026-08-20-03, HIGH) — the two water death sites insert `Dead` without `reconcile_dead_actor`: a drowned actor keeps its AI, keeps its `AnimationPlayer`, and never ragdolls.
- **#3125** (PHYS-D5-2026-08-20-06) — `swim_vertical_velocity`'s damping is per-frame, not per-second: the new swim controller is frame-rate dependent.

The physics audit caught the stale instruction and overrode it. A less skeptical run would have returned "confirmed absent" and lost both.

## Impact

A stale audit baseline that **demonstrably misled an audit inside the last 90 days** — the tech-debt severity table's explicit MEDIUM promotion trigger. The blast radius is every future `/audit-physics` run until fixed, over the subsystem's newest and least-reviewed code.

The class matters more than the instance: a skill file may state *facts* an auditor should verify, but must not issue *instructions to not look*. Facts rot silently; instructions-to-not-look rot silently **and** suppress the evidence that would reveal the rot.

## Note on scope

The physics audit's secondary claim that `watal.md`'s open-items list is *also* stale does **not** hold — `:18-29`, `:218-227` and `:405-425` all say swim/drown are live. The drift is confined to the two audit-infrastructure files above.

## Suggested Fix

1. **Delete `audit-physics/SKILL.md:290-291` outright.** The code is now in scope like any other; there is no "confirm absence" left to do.
2. Update `:60-63` and `_audit-common.md:134` to list the *actual* remaining open items — water-walking, freezing, exact Skyrim DNAM-tail decode, cross-game visual smoke — which is what `watal.md:415-425` already says.
3. Consider a standing convention: audit skills may record **"known-open, do not re-litigate"** facts (with a date), but never **"confirm absence rather than reporting it"**. The former degrades into a false premise an auditor can check; the latter degrades into a blindfold.

## Related

- **#3119**, **#3125** — the two findings this instruction would have suppressed
- `docs/audits/AUDIT_PHYSICS_2026-08-20.md` § "WATAL spec drift worth recording (not a bug)"
- `c7561d74` (shipped the code) · `0c129879` (shipped the instruction)
- `docs/engine/watal.md:22-23`, `:408`, `:617-618` — the spec, already correct

## Completeness Checks
- [ ] **SIBLING**: All three sites updated — `audit-physics/SKILL.md:290-291`, `:60-63`, `_audit-common.md:134`
- [ ] **NO-BLINDFOLD**: No remaining "confirm absence rather than reporting it" instruction in any skill file (`grep -rn "Confirm absence" .claude/commands/`)
- [ ] **SOURCE-OF-TRUTH**: The replacement open-items list is copied from `watal.md:415-425` rather than re-derived
