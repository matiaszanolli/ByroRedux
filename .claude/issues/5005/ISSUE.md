# #5005: FNV-2026-09-29-D2-01: FO3/FNV authored corpses are marked by XRGD ragdoll data, not a header flag — #4814's Starts Dead decode never fires, so every placed corpse spawns alive

**Labels**: bug, high, legacy-compat, gameplay, game:fnv, game:fo3, esm-plugin

**Source report**: `docs/audits/AUDIT_FNV_2026-09-29.md` (FO3 evidence: `docs/audits/AUDIT_FO3_2026-09-29.md`, FO3-D2-01)
**Severity**: HIGH (same impact class as #4814)
**Dimension**: ESM Data Slice. Decode belongs to `/audit-esm`, consumption to `/audit-gameplay`.

## Location
- `crates/plugin/src/esm/cell/walkers.rs` (`PlacedRef` construction): `starts_dead = ACHR && EsmVariant::Tes5Plus && flags & FLAG_STARTS_DEAD`.
- `crates/plugin/src/esm/reader.rs` (`FLAG_STARTS_DEAD = 0x200`, cited from xEdit's TES5 list only).
- `crates/plugin/src/esm/cell/mod.rs` (`PlacedRef::starts_dead` field doc: "FO3/FNV ship none").
- `byroredux/src/cell_loader/references/mod.rs` (actor-job completion) → `byroredux/src/cell_loader/reference_state.rs` (`apply_starts_dead`), the only consumer.

## Description
#4814 (CLOSED) decodes "Starts Dead" only as the ACHR header bit `0x200`. xEdit's FO3/FNV definitions (`wbDefinitionsFNV.pas` / `wbDefinitionsFO3.pas`) give ACHR the record flags {10 Persistent, 11 Initially Disabled, 25 No AI Acquire}, and ACRE adds 15 Visible When Distant. Neither record type has a Starts Dead bit. Both carry `wbRagdoll` (`XRGD` / `XRGB`).

FO3/FNV placed corpses therefore carry no header signal. What they carry is an authored ragdoll pose (`XRGD`). Nothing in the engine reads `XRGD` (0 code hits in `crates/` and `byroredux/`), so `starts_dead` is false for every FO3/FNV actor. These actors go through the normal actor job, get their base's packages and ambient AI, and hostile ones are armed by #4414's faction hostility.

**The rule is inferred from data, not sourced.** "`XRGD` present on an ACHR/ACRE means the actor starts dead" comes from the census below plus xEdit's flag lists. No xEdit or GECK text documents it as "starts dead" (the GECK wiki is Cloudflare-blocked). Per the No Guessing policy, a source must be found before the fix ships.

## Evidence
Byte scan of the vanilla masters (24-byte group headers, compressed bodies inflated).

**FalloutNV.esm**
- Header flag `0x200`: 0 of 3,386 ACHRs and 0 of 2,999 ACREs.
- 387 actor refs carry `XRGD` (278 ACHR, 109 ACRE). 343 of the 387 place a base whose EditorID contains `dead` or `corpse` (`VHDDeadNCRTrooper`, `VHDDeadLegionary`, `VHDDeadCenturion`, `NVProspectorMaleDEADLite`, `NVProspectorMaleDEADSulfurCave03`). Most of the other 44 are deactivated robots (`SSHQProtectronBroken`, `SSHQMrHandyBroken`). Only 14 non-`XRGD` refs place a Dead-named base.
- Header flags on the 387: `0x0` ×209, `0x400` ×146, `0xC00` ×19, `0x800` ×10, so about 358 are not initially disabled and spawn at cell load.
- 29 use bases scripted with `VHDDeadSafetySCRIPT` (`Begin OnLoad / If (GetDead == 0) / Kill`), a safety net for an actor that is normally already dead. The engine has no object-script `Kill` either.

**Fallout3.esm** (from FO3-D2-01)
- `0x200`: 0 of 2,154 ACHR and 0 of 3,349 ACRE.
- 498 actor refs carry `XRGD` (338 ACHR, 160 ACRE); every payload is a multiple of 28 bytes (per-bone pose).
- 428 of the 435 refs whose base is named dead or loot-corpse carry `XRGD` (`DeadBrahmin` ×35, `DeadMoleRat` ×17, `FeralGhoulDEAD` ×14, `DEADGhoulWastelander*` ×24, `DeadSuperMutant1Gun*` ×15, `Loot1*` ~170). The other 70 also read as corpses (`MS16Corpse2/4`, `OasisCorpse`, `AndaleVictim01-03`, `MS18WreckedProtectron01`).
- Base AI is Very Aggressive on 338 of the 498 and Aggressive on 73.

Validated at HEAD 9fcfdc3fc: `walkers.rs` sets `starts_dead` only for `ACHR` + `Tes5Plus` + `0x200`; `rg XRGD crates byroredux` returns 0 hits; the `PlacedRef::starts_dead` doc still says "FO3/FNV ship none".

## Impact
- About 885 authored corpses (387 FNV + 498 FO3) spawn as live NPCs running their packages: the Hoover Dam battlefield dead (Legion corpses are hostile), the Prospector corpses in the caves, the broken Securitron HQ robots, FO3 dead brahmin/ghouls/super mutants and loot corpses.
- They cannot be looted, because `is_loot_source` is gated on `Dead`.
- A hostile corpse starts combat on sight (#4414).
- The #4814 field doc tells the next reader that FO3/FNV have no corpses, which is false.

## Related
- #4814 (CLOSED, Skyrim/FO4 header bit; explicitly scoped FO3/FNV out), #4817, #4693, #4772.
- Sibling per-game decode gaps from the same 2026-09-29 suite (each a distinct marker): see the cross-reference section below.
- AUDIT_ESM_2026-09-29 notes the `0x200` gate is inert on FO3/FNV but does not identify the FO3/FNV marker.

## Suggested Fix
1. First confirm the engine rule (No Guessing policy): find a GECK/xEdit/engine source that `XRGD` presence marks a placed actor as dead on FO3/FNV.
2. Decode `XRGD` presence on ACHR/ACRE for the FO3/FNV variant into `PlacedRef::starts_dead` (or a sibling field). Keep the `0x200` path for Skyrim and later.
3. Correct the field doc.
4. Add real-master census guards (`FalloutNV.esm` `XRGD`-actor count ≥ 380; `Fallout3.esm` = 498).
5. Later, consume the `XRGD` pose as the corpse's initial ragdoll pose (shared with the Skyrim/FO4 pose issue below).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (ACHR and ACRE; FO3 and FNV; the Skyrim/FO4 `0x200` path stays intact)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic ACHR/ACRE with `XRGD`, plus a real-master census guard)

## Cross-references: the starts-dead / starts-unconscious decode family (2026-09-29 suite)
#4814 only knows the Skyrim/FO4 ACHR header bit. Each game below has a distinct marker, so each is filed separately:
- #5005 — FO3/FNV: `XRGD` presence is the only corpse marker (rule **inferred from data, needs a source before fixing**).
- #5013 — Oblivion: base `NPC_`/`CREA` header flag `0x80000` + 0 Health (sourced: xEdit TES4, CS wiki).
- #5015 — Skyrim/FO4: the `XRGD` corpse pose is never decoded (marker is correct; pose is lost).
- #5017 — FO4: ACHR `0x2000` "Starts Unconscious" undecoded (dormant semantics **inferred, needs a source before fixing**).

