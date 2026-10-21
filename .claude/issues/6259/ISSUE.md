# #6259: SF-2026-10-21-D4-01: Essential / protected Starfield actors that go down never get back up — the profile has no bleedout rule, while the data carries one

**Labels**: medium,bug,character,gameplay,esm-plugin,game:starfield,legacy-compat

**Source**: docs/audits/AUDIT_STARFIELD_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: 4 (ESM + bring-up: NPC data through the shared mechanisms; the rule itself belongs to `/audit-character` + `/audit-gameplay`) · **Location**: `crates/core/src/character/profile.rs:461-463` (`STARFIELD.essential_death_time_secs: None`)

## Location
- `crates/core/src/character/profile.rs:461-463` (`STARFIELD.essential_death_time_secs: None`)
- `crates/scripting/src/bleedout.rs:129-160` (`engine_defaults`, `install_bleedout_rules`)
- `crates/scripting/src/bleedout.rs:186-215` (`spare_from_death`)
- `crates/scripting/src/bleedout.rs:218-221` (`bleedout_system` early return)
- `crates/plugin/src/esm/records/actor/race.rs` (Starfield RACE `DAT2` bleedout block not decoded)

## Status at filing
NEW. This is the character audit's unfiled lead, now confirmed. Related open/closed issues:
- #5858 (closed) changed the infinite timer to `None`.
- #6120 (closed) made the flags live.
- CHAR-2026-10-21-L-02 (this suite) covers the #5921 profile rows' missing capture lines.

## Description
- Since #6120 (78facba89), Starfield `NPC_` `ACBS` flags decode. `parse_npc` sets `essential` from bit 1 and `protected` from bit 11 (`npc.rs:584-586`), and `ActorProtection::from_record` stamps them at spawn.
- The Starfield `CharacterRulesProfile` row has `essential_death_time_secs: None`, so `engine_defaults` returns `None`, and `install_bleedout_rules` *removes* `BleedoutRules`.
- When an essential (or non-player-killed protected) actor's health reaches zero, `spare_from_death` inserts `Bleedout { remaining_secs: None }`.
- `bleedout_system` returns immediately without `BleedoutRules`, so the actor stays in bleedout (counted unconscious by every #5017 consumer) until a script `Resurrect` (`systems/revive.rs`).
- The state also survives cell eviction (`reference_state.rs`) and saves (`save_io/registry.rs:382`).

The profile comment's rationale ("Starfield's executable carries no `fEssentialDeathTime`" → "the game has no bleedout recovery", `profile.rs:245-249` / `bleedout.rs:133-135`) is contradicted by the game's own data.

## Evidence
- **`Starfield.exe` strings**:
  - `fEssentialHealthPercentReGain`, `fEssentialHostileDeathTimeMult`, `fEssentialNPCMinimumHealth`, `fSpaceshipEssentialDeathTime`;
  - the Papyrus surface `SetNoBleedoutRecovery` / `GetNoBleedoutRecovery` / `NoBleedoutRecovery`, and the event `OnRecoverFromBleedout`.
  - There is no `fEssentialDeathTime` string; `Fallout4.exe` has one.
- **xEdit `wbDefinitionsSF1.pas`**:
  - RACE `DAT2` +160..+184 is `Bleedout { Health %, Chance, Recover Chance, Time Min, Time Max, Health Drain Rate }` (lines 16582-16589).
  - `NPC_` `ONA2` flag 5 carries per-NPC `Bleedout Data` overrides (15358-15365).
  - `ACBS` bit 16 is "Doesn't bleed" and bit 18 is "Bleedout Override" (15016-15050).
- **Probe (`/tmp/audit/starfield/probe/npc_race.py`, read-only over `Starfield.esm`)**:
  - 7,131 `NPC_` `ACBS`: 626 Essential, 20 Protected, 87 "Doesn't bleed".
  - 31 RACE `DAT2` ≥ 184 B. `HumanRace` = (0.25, 0.25, 0.25, **8.0, 12.0**, 0.01). 15 races (`ModelARace`, `MiniBotARace` among them) author all zeros.

## Impact
any Starfield essential actor (companions, quest givers) brought to zero health by combat, a hazard or a script `DamageValue` stays down permanently: no AI, no dialogue, no packages. The state is persisted, so the save is poisoned too. Before #6120 the flags were 0, so this path was unreachable; the flag fix exposed it.

## Related
#6120, #5858, #5921 (profile rows), CHAR-2026-10-21-L-02.

## Suggested Fix
- Do not invent a value (No Guessing policy).
- Decode the SF RACE `DAT2` bleedout block and the `ONA2` override so the data is captured.
- Source the recovery semantics (how `Time Min/Max`, `Recover Chance`, `Health Drain Rate` and `fEssentialHostileDeathTimeMult` combine) from a CK / Papyrus capture before wiring a Starfield `BleedoutRules`.
- Until then, correct the profile and module docs, which claim Starfield has no recovery, and log once when an actor enters a recovery-less bleedout on a game whose rule is unsourced (as opposed to genuinely absent).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
