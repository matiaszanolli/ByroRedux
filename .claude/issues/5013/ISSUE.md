# #5013: OBL-2026-09-29-D2-01: Oblivion marks corpses on the base NPC_/CREA record (header flag 0x80000 "Starts Dead", authored with 0 Health), not on the placement — all 787 placed Oblivion corpses spawn alive

**Labels**: bug, high, legacy-compat, gameplay, game:oblivion, esm-plugin

**Source report**: `docs/audits/AUDIT_OBLIVION_2026-09-29.md`
**Severity**: HIGH (same impact class as #4814 and #5005)
**Dimension**: BSA v103 & ESM Data Slice. Decode belongs to `/audit-esm`, consumption to `/audit-gameplay`; the Oblivion data half is owned here.

## Location
- `crates/plugin/src/esm/cell/walkers.rs` (`PlacedRef` construction): `starts_dead = ACHR && Tes5Plus && flags & 0x200`, a placement-only signal.
- `crates/plugin/src/esm/reader.rs` (`FLAG_STARTS_DEAD = 0x200`, cited from the TES5 list only).
- `crates/plugin/src/esm/cell/mod.rs` (`PlacedRef::starts_dead` doc: "Oblivion is excluded because … no Oblivion source was checked").
- `crates/plugin/src/esm/records/actor/mod.rs`: the `NPC_`/`CREA` parsers keep no record-header flags, and the Oblivion `NPC_` `DATA` arm reads no health.
- `byroredux/src/cell_loader/reference_state.rs` (`apply_starts_dead`), called from the actor-job completion in `byroredux/src/cell_loader/references/mod.rs`.

## Description
#4814 decodes "Starts Dead" as ACHR record flag `0x200` on the 24-byte header family only. TES4 puts "Starts Dead" on the **base actor** instead: xEdit's TES4 flag lists give `19, 'Starts Dead'` for both `CREA` (`wbDefinitionsTES4.pas:1942`) and `NPC_` (`:2783`), i.e. record-header bit `0x80000`. The Construction Set's authoring rule is the same fact from the tool side: a base actor with 0 Health spawns dead.

The engine decodes neither the base flag nor the base health, so every Oblivion actor placement spawns alive.

Unlike the FO3/FNV rule (#5005), this one is **sourced**: xEdit TES4 plus the CS wiki ("Stats Tab - Creatures", cs.uesp.net: *"If you specify 0 health for a creature, it will automatically spawn in the cell as dead, just as NPCs do."*). Health layouts: UESP `Oblivion_Mod:Mod_File_Format/NPC_` gives `DATA` Health as `ulong` @21; `/CREA` gives it as `ushort` @6.

## Evidence
Raw census of `Oblivion.esm`:
- `0x200` appears on 0 of 2,190 ACHR and 0 of 1,473 ACRE (flags seen: `0x400`, `0x800`, `0x8000`).
- Base flag `0x80000` is set on 135 bases (87 `NPC_`, 48 `CREA`). 132 of them also author 0 Health; 3 `CREA` have non-zero health. No unflagged base authors 0 Health.
- Refs placing a flagged base: 156 ACHR + 631 ACRE = **787**; 771 of them carry `XRGD`. Only 4 `XRGD` refs sit on an unflagged base.
- Examples: `MS12SkeletonDeadState` `CREA` flags `0x80000`, `DeadSkeleton` `0x80400`, `DeadBanditMale01` `NPC_` `0xC0400`.
- Top bases: DeadSkeleton 253, DeadSkeleton2 108, DeadZombieHeadless 49, DeadZombie 37, RatDead 30, SE09DeadFailedExperiment03 29, MS12SkeletonDeadState 24, DeadGoblin 15.

Validated at HEAD 9fcfdc3fc: `walkers.rs` gates `starts_dead` on `Tes5Plus` + `ACHR` + `0x200`; no `0x80000` / record-flag capture exists under `crates/plugin/src/esm/records/actor/`; the `PlacedRef::starts_dead` doc still excludes Oblivion.

## Impact
- Every authored Oblivion dungeon corpse (skeleton piles, dead adventurers and bandits, Shivering Isles experiment and war dead) spawns as a live actor.
- It stands or idles, runs its AI packages, and is offered as a container rather than a corpse (`interaction.rs` keys `InteractionKind::Corpse` on `Dead`).
- Its `XRGD` pose is also unused (the pose half is tracked with the Skyrim/FO4 issue, see cross-references).

## Related
- #4814 (CLOSED), #5005 (FNV-2026-09-29-D2-01, FO3/FNV `XRGD` marker), LEGACY_COMPAT 2026-09-29 Dim 3 census (showed the `0x200` exclusion drops nothing, did not establish the Oblivion marker).

## Suggested Fix
- At the parse boundary, keep the Oblivion `NPC_`/`CREA` record-header bit `0x80000` ("Starts Dead", per xEdit TES4) on the actor record.
- Derive `starts_dead` for Oblivion placements whose resolved base carries it. The `PlacedRef` cannot see its base, so do this at the translate step that already resolves `record_index.actor`, not in the walker.
- Correct the `PlacedRef::starts_dead` field doc.
- Pin a real-master census: 135 flagged bases, 787 corpse refs.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`NPC_` and `CREA`; leveled bases resolving to a flagged actor)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic flagged base + real-master census)

## Cross-references: the starts-dead / starts-unconscious decode family (2026-09-29 suite)
#4814 only knows the Skyrim/FO4 ACHR header bit. Each game below has a distinct marker, so each is filed separately:
- #5005 — FO3/FNV: `XRGD` presence is the only corpse marker (rule **inferred from data, needs a source before fixing**).
- #5013 — Oblivion: base `NPC_`/`CREA` header flag `0x80000` + 0 Health (sourced: xEdit TES4, CS wiki).
- #5015 — Skyrim/FO4: the `XRGD` corpse pose is never decoded (marker is correct; pose is lost).
- #5017 — FO4: ACHR `0x2000` "Starts Unconscious" undecoded (dormant semantics **inferred, needs a source before fixing**).

