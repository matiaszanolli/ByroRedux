# DIM9-01: parse_npc never remaps embedded FormIDs (PKID/ai_packages) to global load-order space

- **Severity**: HIGH
- **Dimension**: AI Packages & Sandbox Behavior (FNV audit, Dimension 9)
- **Location**: `crates/plugin/src/esm/records/actor.rs:493` (`parse_npc` signature, no `remap` param), `:580-584` (`PKID` arm), `crates/plugin/src/esm/records/mod.rs:480-487` (call site, no remap threaded)
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1996
- **Source report**: `docs/audits/AUDIT_FNV_2026-07-15_DIM9.md`

## Description
Every other per-record parser that carries embedded FormID references explicitly remaps them from plugin-local to global load-order space via `reader.get_form_id_remap()` — `parse_pack`, `parse_qust`, `parse_perk`, `parse_avif`, `parse_dial`, `parse_info` all take a `remap: &Option<FormIdRemap>` parameter. `parse_npc` has no such parameter, and the `mod.rs` NPC_ call site never obtains or threads a remap. Every embedded FormID field on `NpcRecord` (including `PKID` → `ai_packages`) is stored raw.

`EsmIndex.packages` is keyed by properly-remapped global FormIDs, so `npc_spawn.rs`'s `npc.ai_packages.iter().filter_map(|pk| index.packages.get(pk))` compares an unremapped local `PKID` against a remapped global key.

## Impact
On any multi-plugin FNV load (base + DLC, base + mod), every non-base-plugin NPC's package lookups silently miss — `active_package_is_sandbox` returns `false` unconditionally and that NPC never sandboxes. No crash, no log. Same root cause affects race/class/voice/faction/outfit/death-item/template/inventory FormIDs on `NpcRecord` too.

## Suggested Fix
Add a `remap: &Option<FormIdRemap>` parameter to `parse_npc`, thread `reader.get_form_id_remap()` through at the `mod.rs:486` call site (mirroring the PACK/QUST/PERK arms immediately below it), and apply remapping to every embedded FormID field in the sub-record loop.

## Related
Shares the remap mechanism introduced for #1666 — `parse_npc` is the one holdout never updated to that pattern.
