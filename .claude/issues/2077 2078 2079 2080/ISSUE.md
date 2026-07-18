# Issue batch: 2077, 2078, 2079, 2080

## #2077 — TD8-103: npc_spawn.rs's two pub use re-exports claim existing call sites that don't exist
- Severity: LOW (bug, tech-debt)
- Location: `byroredux/src/npc_spawn.rs:29-33,431-435`
- Both `pub use` re-exports (`Gender`, `normalize_mesh_path`) carry comments justifying external call sites that don't exist — single binary crate, no external consumers.
- Suggested fix: change both to plain `use`; delete the misleading comments.

## #2078 — FNV-D1-02: NifImportRegistry cache key has no plugin/archive-set discriminant — stale cross-load reuse via debug cell.load
- Severity: MEDIUM (bug, import-pipeline)
- Location: `byroredux/src/cell_loader/nif_import_registry.rs:136-180`, `byroredux/src/cell_loader/references/mod.rs:610-617`, `byroredux/src/boot.rs:359`, `byroredux/src/debug_load.rs:206-266,268-360`
- `NifImportRegistry` caches by lowercased model path only, no archive-set discriminant, no `clear()` method. `debug_load.rs`'s `exec_load_interior`/`exec_load_exterior` never invalidate it (or `TextureRegistry::path_map`, same pattern) when the requested `--bsa`/`--esm`/`--master` set changes between debug `cell.load` invocations — stale content silently served.
- Suggested fix: fold an archive-set identity/generation counter into the cache key, or clear the registries when the archive set changes.

## #2079 — FNV-D4-01: parse_otft / parse_leveled_list (LVLI/LVLN) / parse_container (CONT) never remap embedded FormIDs
- Severity: HIGH (bug, import-pipeline)
- Location: `crates/plugin/src/esm/records/outfit.rs:44-61` (INAM), `crates/plugin/src/esm/records/container.rs:80-136` (CNTO), `:139-169` (LVLO); call sites `crates/plugin/src/esm/records/mod.rs:452-480,743-745`; consumer `crates/plugin/src/equip.rs:304-373`
- Same root cause as closed #1996 (NPC_ FormID remap) but for OTFT/LVLI/LVLN/CONT — these parsers never obtain/thread a `FormIdRemap`, so embedded FormIDs from non-base plugins silently fail to resolve on multi-plugin loads (naked NPCs, empty containers).
- Suggested fix: add `remap: &Option<FormIdRemap>` to all three parsers, thread through at `mod.rs` call sites, remap INAM/LVLO.form_id/CNTO.item_form_id.

## #2080 — FNV-D4-02: parse_npc's FaceGen-recipe FormID fields (HNAM/ENAM/PNAM-eyebrow/FMRI) still unremapped after #1996
- Severity: HIGH (bug, import-pipeline)
- Location: `crates/plugin/src/esm/records/actor.rs:691-712,754-763`; consumer `byroredux/src/npc_spawn.rs:1083-1165`
- #1996 added `remap` to `parse_npc` and applied it to most fields but missed HNAM/ENAM/PNAM-eyebrow/FMRI (FaceGen recipe fields) — still raw `u32_or_default()` with no `remap_fid()` wrapper.
- Suggested fix: wrap HNAM/ENAM/PNAM(both arms)/FMRI reads in the existing local `remap_fid(raw, remap)` helper.
