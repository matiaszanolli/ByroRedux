# #7108 — ESM-D3-03: Oblivion `MGEF` `DATA` @8 remapped as a FormID when it is an actor-value enum (`Use actor value`)

Labels: low,esm-plugin,bug,game:oblivion
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D3-03)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D3-03**

- **Severity**: LOW (latent: the enum value is consumed correctly via `primary_actor_value`; the corrupted copy has no consumer)
- **Dimension**: 3
- **Record / Sub-record**: Oblivion `MGEF` / `DATA` @8
- **Location**: `crates/plugin/src/esm/records/misc/magic.rs:1358` (`out.associated_item = remap_fid_or_sentinel(header.associated_item, remap)`, ungated by game or flags); correct raw read at the Oblivion arm (`index_ref(read(8))`, `magic.rs` Oblivion branch with `USE_ACTOR_VALUE`).
- **Status**: NEW (the raw AV read arrived with `278886579`, "Enhance Oblivion Actor Values Implementation"; the unconditional remap is older, and was only unobservable while nothing read the same word as an AV).
- **Description**: xEdit TES4 `wbMGEFFAssocItemDecider` (`wbDefinitionsTES4.pas:818-836`) picks a FormID (weapon / armor / creature) only for flag bits 16/17/18, an `itS32 wbActorValueEnum` for bit 24 (`Use actor value`), and 4 unknown bytes otherwise. The decoder now knows this (it reads the AV enum for the flag), yet still remaps the same word.
- **Evidence**: Oblivion.esm MGEF census: 59 `Use actor value` effects with `DATA@8` in {0x06, 0x21, 0x22, 0x46, ...}; 25 "unknown4" effects all 0. A raw `9` on a masterless plugin at slot N>0 becomes `0x0N000009`, and any enum with top byte 0 on a plugin whose first master is not at slot 0 shifts the same way. Vanilla (slot 0) is identity.
- **Impact**: `MgefRecord.associated_item` is wrong for mod-added Oblivion AV effects on a non-trivial load order. Unconsumed today (`cast_effect` reads it only for archetype `Other(1)` script effects), so latent.
- **Suggested Fix**: on Oblivion remap `associated_item` only when flag bits 16-18 are set; keep the raw word otherwise. Pin with a non-identity-remap MGEF test beside `parse_mgef_associated_item_and_effect_shader_id_are_remapped`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
