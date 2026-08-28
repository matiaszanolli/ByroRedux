# #3417 — RT-2026-08-27-05: the default p2-melee-core gate has never passed — its Skyrim weapon-family assertion was unsatisfiable at the commit that authored it

Labels: medium, test-gap, combat, game:skyrim, bug
Filed: 2026-08-27 by `/audit-publish docs/audits/AUDIT_RUNTIME_2026-08-27.md`
Source report: `docs/audits/AUDIT_RUNTIME_2026-08-27.md`

---

Source: `docs/audits/AUDIT_RUNTIME_2026-08-27.md` — RT-2026-08-27-05 (live gate runs at `969d81c8`).

- **Severity**: MEDIUM
- **Dimension**: playable-slice smoke gates (un-owned subsystem)
- **Location**: `docs/smoke-tests/fixtures/skyrim_se.env:87-90`; assertion at `docs/smoke-tests/p2-melee-core.sh:112-115`

## Description

`p2-melee-core.sh` with no argument selects `skyrim_se` (`docs/smoke-tests/lib/fixture.sh:45`), and fails immediately at the ESM preflight:

```
smoke[p2-melee-core]: FAIL -- weapon leaf 0001CB64:DraugrBattleAxe:damage=18 is absent from the fixture family
```

The fixture pins two leaves for `BleakFallsBarrow01`:

```sh
P2_PROBE_WEAPON_LINES=(
    "0001CB64:DraugrBattleAxe:damage=18"
    "000236A5:DraugrGreatsword:damage=17"
)
```

and the gate `grep -Fq`s each one or fails. Only `000236A5:DraugrGreatsword:damage=17` is produced. Across the whole cell the probe emits `6x DraugrGreatsword:damage=17`, `3x DraugrWarAxe:damage=9`, and **zero** `DraugrBattleAxe`. The frozen target `000383F7` / `EncDraugr01AmbushMelee2HHeadM06` resolves to the Greatsword.

## Evidence

`probe_combat_fixture` was rebuilt and run at **`3aebf414`**, the commit that introduced `fixtures/skyrim_se.env`, and produces the same three weapon lines with no `DraugrBattleAxe`. The assertion was therefore never satisfiable — the gate has been deterministically RED since it was authored on 2026-08-27, and its engine phase (character mode, hit chain, Health→`Dead`, 18-body ragdoll) has never executed on the default arm.

## Impact

The gameplay slice has no owner audit skill; these gates are its only coverage. A gate that fails before launching the engine provides none, and a red-by-default gate trains readers to ignore it. The skill's own reference text still describes P2 as "passing as of 2026-08-16" — that predates the #3039 fixture parameterisation and is now wrong for the default arm.

## Related

#3039 (fixture parameterisation), `3aebf414`. #3423 is the other arm of the same gate. `AUDIT_RUNTIME_2026-08-16.md` found the same *class* of defect (gates deterministically red from assertion drift) in two other gates.

## Suggested Fix

Re-derive `P2_PROBE_WEAPON_LINES` from a live `probe_combat_fixture` run rather than by hand, or drop the second leaf. Worth asking separately whether the leveled weapon list for `000E9895` *should* still reach the battle-axe leaf — if it should, the fixture is right and the LVLI expansion is the defect.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other frozen-line assertions in `p0`/`p1`/`p2` fixtures, both game arms)
- [ ] **TESTS**: A regression test pins this specific fix
