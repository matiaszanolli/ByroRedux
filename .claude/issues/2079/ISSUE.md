# FNV-D4-01: parse_otft / parse_leveled_list (LVLI/LVLN) / parse_container (CONT) never remap embedded FormIDs

- **Severity**: HIGH
- **Labels**: high, import-pipeline, bug
- **Location**: `crates/plugin/src/esm/records/outfit.rs:44-61` (INAM), `crates/plugin/src/esm/records/container.rs:80-136` (CNTO), `:139-169` (LVLO); call sites `crates/plugin/src/esm/records/mod.rs:452-480,743-745`; consumer `crates/plugin/src/equip.rs:304-373`
- **Status note**: same root cause as closed #1996/DIM9-01, different un-fixed parsers.

## Description
#1996 (closed 2026-07-15) added FormID remapping to `parse_npc` because embedded FormID fields inside a record's sub-records are stored plugin-local and need explicit remapping via `reader.get_form_id_remap()` — only the record's own key is auto-remapped. The fix touched `parse_npc` only. `parse_otft`, `parse_leveled_list` (LVLI/LVLN), and `parse_container` (CONT) still read raw, unremapped FormIDs and are never given a `FormIdRemap` at their `mod.rs` call sites — in contrast with the adjacent PACK/QUST/PERK/AVIF arms, which do obtain one via `reader.get_form_id_remap()` and thread it through.

## Evidence
`parse_otft` (outfit.rs:44), `parse_cont`, and `parse_leveled_list` (container.rs) all take no `remap` parameter and read `INAM`/`CNTO.item_form_id`/`LVLO.form_id` via plain `u32_or_default()`/sub-reader calls with no `remap_fid()` wrapper. Their `mod.rs` call sites (CONT ~452, LVLI/LVLN ~463-470, OTFT ~743) pass no remap either.

## Impact
On any FNV load with more than one plugin (the *normal* case — this engine's own multi-master CLI, or any real playthrough with a DLC master), OTFT/LVLI/LVLN/CONT records authored in a non-base plugin whose FormID references point at content defined in that same plugin resolve against the wrong (global-keyed) map and silently miss. `expand_leveled_form_id` treats an unresolvable FormID as "not an item, not a list" and pushes nothing — affected NPCs spawn naked/gearless, affected loot containers spawn empty, no error logged. Undetected by existing tests because a standalone-master load has an identity remap.

## Suggested Fix
Add `remap: &Option<FormIdRemap>` to all three parsers, thread `reader.get_form_id_remap()` through at their `mod.rs` call sites (mirroring the PACK/QUST/PERK/AVIF/NPC_ pattern), remap `INAM`/`LVLO.form_id`/`CNTO.item_form_id`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other record parsers sharing embedded-FormID sub-records)
- [ ] **SIBLING-PARSERS**: Same remap/fix applied to all sibling record parsers that share the same field pattern (not just the one parser originally patched) — specifically `parse_otft`, `parse_leveled_list`, and `parse_container` all in one pass, matching the PACK/QUST/PERK/AVIF/NPC_ convention
- [ ] **TESTS**: A regression test pins this specific fix (multi-plugin load with OTFT/LVLI/LVLN/CONT records referencing same-plugin content)
