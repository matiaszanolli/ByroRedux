# PEX-D3-2026-09-19-01: EVENT_NAMES is missing real engine events — 20 vanilla handlers demote to Function

- **ID**: D3-01
- **Labels**: medium,scripting,bug
- **Filed from**: docs/audits/AUDIT_PAPYRUS_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4471

**Severity**: MEDIUM · **Dimension**: Decompiler Boolean/Control-Flow/Lower (event classification) · **Untrusted-Input**: Yes
**Source**: `docs/audits/AUDIT_PAPYRUS_2026-09-19.md` (PEX-D3-2026-09-19-01) · **Location**: `crates/pex/src/decompile/event_names.rs:13-281`; consumed at `crates/pex/src/decompile/lower.rs:295-297`

**Description**
The event-classification list is missing engine events that vanilla handlers actually implement, so the `on`-prefix AND `is_event_name` rule (`lower.rs:295-297`) demotes those handlers to `Function` items in the decompiled AST. A full-corpus census (26,641 scripts; 12,096 `on`-prefixed functions) found 33 distinct demoted names, of which 12 names / 20 handlers are engine events per the strongest available signal — implemented in base-class scripts (`actor.pex`, `spaceshipreference.pex`, `quest.pex`, `scriptobject.pex`, `referencealias.pex`, `activemagiceffect.pex`) that can only override engine hooks:
- Skyrim: `OnAttach` ×2 (`dlc1ld_ghostscript.pex`, `dlc1fadingghostscript.pex`)
- FO4: `OnStoryClearLocation` ×1
- Starfield: `OnShipCruiseArrival` ×4, `OnGameplayOptionChanged` ×3, `OnUnconscious` ×2, `OnStoryChangeLocationEx` ×2, `OnSpaceshipCombatListAdded`/`Removed` ×2, `OnPlayerFastTravel` ×1, `OnStorySpeechChallengeCompletion` ×1, `OnPlayerScanPlanet` ×1, `OnPlayerShip` ×1

The other 21 demoted names are correct demotions (vanilla source typos like `oncelldetatch`, script-defined `oncritter*`) — not in scope.

**Evidence**
Census probe over the Skyrim SE / FO4 / Starfield corpus archives; absent names grep-confirmed in `event_names.rs`. The header pins the list to Champollion's `EventNames.hpp` ("GENERATED … do not hand-edit"), but the shipped list already contains Starfield events beyond Champollion v1.3.2, so its generation source is a Starfield-era union that is itself incomplete.

**Impact**
~0.17% of `on`-prefixed vanilla functions (20 of 12,096; 17 of them Starfield) come out as `Function` instead of `Event` — bodies and names survive, but any recognizer or round-trip consumer keying on event-ness misses them, and a `.psc` rendering loses the `Event` keyword. Fidelity gap, not robustness.

**Related**: #3786, #3943 (adjacent classification fixes, closed)

**Suggested Fix**
Regenerate the union from a complete per-game event dump (the game's own source `.psc` event set or the CK/Starfield creation-kit event index) instead of Champollion's frozen lists, keeping the sorted+lowercase+binary-search invariant and the `list_is_sorted_for_binary_search` guard; add the 12 missing names to the smoke test's known-event assertions.

## Completeness Checks
- [ ] **SIBLING**: Check whether any recognizer or tooling keys on event-ness and needs the same 12 names
- [ ] **TESTS**: A regression test pins that the 12 engine events classify as `Event`
