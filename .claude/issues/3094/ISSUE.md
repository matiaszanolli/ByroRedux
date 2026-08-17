# CHAR-2026-08-16-D2-02: FALLOUT_FO3_FNV spells two skills with display names and resolves a retired one

**Issue**: #3094
**Severity**: MEDIUM
**Labels**: `medium,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_CHARACTER_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_CHARACTER_2026-08-16.md` (Dimension 2 — per-game ruleset fidelity).

**Location**: `crates/core/src/character/skill.rs`:151-176 (`SkillSet::FALLOUT_FO3_FNV`)

## Description

`SkillSet::FALLOUT_FO3_FNV` spells two FNV skills with their **display** names rather than their `AVIF` EditorIDs, and resolves one that FNV retired.

## Evidence

The live roster (re-verified 2026-08-17):
```
"Barter" "EnergyWeapons" "Explosives" "Guns" "Lockpick" "Medicine"
"MeleeWeapons" "Repair" "Science" "Sneak" "Speech" "Survival"
"Unarmed" "SmallGuns" "BigGuns"
```

`SmallGuns` and `BigGuns` are the **FO3** skills — FNV replaced them with `Guns` and `Explosives`. Carrying both means the FNV roster resolves a retired skill, and two entries are spelled as display names rather than as the EditorIDs the lookup actually needs.

## Impact

Resolution failures are silent: `AttributeSet::resolve` / `SkillSet::resolve` `filter_map` unresolved ids away (`crates/core/src/character/attribute.rs`:147-152), so a mis-spelled or retired entry simply vanishes from the built `CharacterRuleset` with no diagnostic.

Today this is masked entirely by #2986 — the `AV` prefix means *nothing* resolves on FNV — so it will only become visible once that lands. Worth fixing in the same pass so the roster is correct when resolution starts working.

## Suggested Fix

Split the FO3 and FNV rosters (they are different skill sets), and spell every entry as its `AVIF` EditorID. Cross-check against `docs/engine/charal-fnv-fo3-ruleset.md`, which is the authority.

Consider making unresolved roster entries **loud** rather than `filter_map`-silent — that is what would have surfaced this.

## Related

- **#2986 (ESM-D7-01 — currently masks this entirely; fix together)**
- `docs/engine/charal-fnv-fo3-ruleset.md`

## Completeness Checks
- [ ] **PER-GAME**: FO3 and FNV rosters separated — they are not the same skill set
- [ ] **EDITOR-IDS**: Every entry is the `AVIF` EditorID, verified against shipped masters
- [ ] **NOT-SILENT**: An unresolved roster entry logs rather than being `filter_map`ed away
- [ ] **CO-RESOLVE**: Fixed alongside #2986, which currently hides the symptom
- [ ] **TESTS**: A real-data test asserts every FNV roster entry resolves

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3094 --json state` when live state is needed.*
