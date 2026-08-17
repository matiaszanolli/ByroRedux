# ESM-2026-08-16-D7-01: AVIF EditorIDs are AV-prefixed on FO3/FNV/Skyrim, so actor-value population resolves nothing

**Issue**: #2986
**Severity**: HIGH
**Dimension**: 7 — EsmIndex → ECS Handoff
**Labels**: `high,import-pipeline,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_ESM_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_ESM_2026-08-16.md` (Dimension 7 — `EsmIndex` → ECS Handoff, corroborated by Real-Data Validation).

**Record / Sub-record**: `AVIF` / `EDID`, `NPC_` / `ACBS`
**Location**: `crates/plugin/src/esm/records/index.rs`:575-591 (`actor_value_form_id` + its docstring) · `crates/plugin/src/esm/records/actor_value_derive.rs`:180-206 (`derive_autocalc_actor_values`)

## Description

`actor_value_form_id` resolves an actor value by exact (case-insensitive) `AVIF` EditorID. It is queried with the canonical CHARAL roster strings — `Attribute::editor_id()` returns `"Strength"`, `SkillDef::editor_id` returns `"Guns"`, `"Sneak"`, … — and its own docstring asserts *"the EditorIDs are the GECK names (`"Strength"`, `"Sneak"`, `"Guns"`, …)"*.

**That is true of Fallout 4 and of no other shipped game.** Fallout 3, Fallout New Vegas and Skyrim SE prefix **every** `AVIF` EditorID with `AV`.

Every `if let Some(fid) = index.actor_value_form_id(…)` in `derive_autocalc_actor_values` therefore takes the `None` arm, the function returns an empty `Vec`, and the FO3/FNV auto-calc path — the #1663 reference path this whole module exists for — is **dead on 100% of real data**.

## Evidence

Throwaway probe (`crates/plugin/examples/audit_esm_av_probe.rs`, run and removed), release build, masters per `_audit-common.md` § Game Data Locations:

```
=== FalloutNV.esm ===
avif records = 64
sample avif edids: ["AVActionPoints", "AVAggression", "AVAgility",
                    "AVAssistance", "AVBarter", "AVBigGuns", ...]
  actor_value_form_id("Health")     = None
  actor_value_form_id("AVHealth")   = Some(1104)
  actor_value_form_id("Strength")   = None
  actor_value_form_id("AVStrength") = Some(1000)
  health_actor_value_key            = None
NPC_ = 3816 | derived-empty = 3816 | derived-nonempty = 0

=== Fallout3.esm ===   NPC_ = 1647 | derived-empty = 1647 | derived-nonempty = 0
=== Fallout4.esm ===   actor_value_form_id("Health") = Some(724)
                       NPC_ = 3015 | derived-empty = 0 | avg pairs = 12.3
```

An independent raw-bytes walk of the top-level `AVIF` GRUP agrees: `FalloutNV.esm` ships `0x3e8 AVStrength`, `0x450 AVHealth`, `0x4b9 AVSmallGuns`, `0x44c AVActionPoints`; `Fallout4.esm` ships `0x2d4 Health`, `0x2d5 ActionPoints`.

Re-verified 2026-08-17: the docstring still names the bare GECK spellings, the lookup is still a bare `eq_ignore_ascii_case`, and `grep -rl "AVStrength\|AVHealth"` over `docs/ crates/ byroredux/` returns **only today's audit reports** — nothing in the engine mentions the prefix.

## Impact

Blast radius traced to four live consumers, all FO3/FNV-wide:

1. `byroredux/src/npc_spawn.rs`:100 — `pairs.is_empty()` returns early, so the actor gets **neither** `ActorValues` **nor** `ActorVitals`.
2. `byroredux/src/combat.rs`:200 requires `ActorVitals`; without it the melee damage system returns before touching the target. **No FO3/FNV actor can be damaged by the P2 combat slice.**
3. `crates/scripting/src/condition.rs`:429 — `GetActorValue` with no `ActorValues` component returns `0.0`, the absent-AV default, for every CTDA on every FO3/FNV actor.
4. `byroredux/src/npc_spawn.rs`:197 — `build_character_ruleset` passes the same resolver into `falloutnv_ruleset`; `AttributeSet::resolve` / `SkillSet::resolve` `filter_map` unresolved ids away (`crates/core/src/character/attribute.rs`:147-152), so the FNV `CharacterRuleset` is built with an **empty roster**.

FNV is the project's declared reference title (`/audit-fnv`), which makes this a silent zero on the game the engine is calibrated against.

## Suggested Fix

Resolve **per-game** rather than by one literal: try the bare EditorID and the `AV`-prefixed form (FO3/FNV/Skyrim author the latter, FO4+ the former), and pin it with a `#[ignore]` real-data test asserting `actor_value_form_id("Strength")` is `Some` on `FalloutNV.esm`.

Do **not** fix it by re-spelling the CHARAL roster — the roster is canonical and shared with FO4, where the bare names are correct. The per-game spelling belongs at this parser-side boundary.

## Related

- Root cause shared with ESM-2026-08-16-D7-02 — **fix them together**
- Test-fixture tautology is the same defect class as ESM-D2-07 (2026-08-13)
- Hand-off to `/audit-character` Dim 4 — CHARAL's FNV ruleset is unwired against real data independent of anything CHARAL does
- Not the same as ESM-D8-04 (`RACE`/`NPC_` sub-code coverage)

## Completeness Checks
- [ ] **SIBLING**: Every other EditorID-keyed lookup in `index.rs` checked for the same per-game spelling assumption
- [ ] **CANONICAL-BOUNDARY**: The per-game spelling lives at the parser boundary — the CHARAL roster stays canonical and unmodified
- [ ] **REAL-DATA**: A `#[ignore]` test asserts resolution against `FalloutNV.esm`, not a synthetic fixture (the fixture is what hid this)
- [ ] **NO-GUESSING**: The `AV` prefix set is measured from shipped masters, not assumed uniform across FO3/FNV/Skyrim
- [ ] **TESTS**: A regression test pins this specific fix

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 2986 --json state` when live state is needed.*
