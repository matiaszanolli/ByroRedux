# #7109 — ESM-D3-04: CTDA fn 14 `GetActorValue` `param_1` remapped on Oblivion/FO3/FNV/Skyrim, where it is an AV enum

Labels: low,esm-plugin,scripting,bug
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D3-04)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D3-04**

- **Severity**: LOW
- **Dimension**: 3
- **Record / Sub-record**: any record carrying a CTDA (effect-item, INFO, PERK, PACK, QUST ...), Oblivion / FO3 / FNV / Skyrim
- **Location**: `crates/plugin/src/esm/records/condition.rs:542-545` (`14 // GetActorValue — AVIF FormID` in `param1_is_form_id`), applied unconditionally by `remap_condition_form_ids` (`:628-641`, no `game` parameter).
- **Status**: NEW for the plugin side. Distinct from closed #5989, which fixed the *evaluator* (`condition_actor_value_form`, `actor_value_names.rs:273-305`) and explicitly leaves the remap in place on the assumption that small indices pass through unchanged.
- **Description**: xEdit types `GetActorValue`'s `ptActorValue` as `wbActorValueEnum` on TES4/FO3/FNV/TES5 and as an AVIF FormID only on FO4+ (`wbDefinitionsFO4.pas:270`; `catalog.rs:750` `authored_actor_value_is_enum_code`). The remap table has no game axis, so an authored enum code is run through `FormIdRemap::remap`.
- **Evidence**: `remap(9)` with masters `[slot 3]` composes `0x03000009`; with no masters at slot N, `0x0N000009`. Identity only when the first master sits at slot 0. The evaluator then cannot translate it: Oblivion `condition_actor_value_form` requires `param_1 <= 0x00FF_FFFF` for `vanilla_name`, Skyrim/FO3/FNV index tables miss — the fail-closed gates then decline the condition. Same hit for effect-item conditions after `EFIT` (e.g. FNV `PlayerConcussed`: `GetActorValue 25`, which the test `parse_spel_keeps_each_effects_own_conditions` runs with `&None`, so it cannot catch this).
- **Impact**: Mod-authored AV conditions on a plugin whose first master is not slot 0, or a masterless plugin loaded after slot 0, are never satisfied. Small blast radius; vanilla unaffected.
- **Related**: #5989, #6558; the `param1_form_id_table_matches_the_condition_catalog` guard cannot see this (it walks the catalog's indices, not game semantics).
- **Suggested Fix**: thread the game into the CTDA remap (or into `push_ctda`) and skip `param_1` for fn 14 and the other `ptActorValue` indices (109, 277, ...) unless the game is FO4 or later; or have the evaluator accept both. Add a non-identity-remap test for Oblivion fn 14.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
