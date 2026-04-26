# FNV-D5-LOW: Observability + docs polish — ROADMAP grid copy, texture_count scope, --cmd headless, unresolved REFR bases

## Finding: FNV-D5-LOW (bundle of FNV-D5-01 / 02 / 03 / 04)

- **Severity**: LOW (all)
- **Source**: `docs/audits/AUDIT_FNV_2026-04-24.md`

## FNV-D5-01: ROADMAP "Exterior 3×3" copy is ambiguous

**Location**: [ROADMAP.md:33, 73](ROADMAP.md#L33).

ROADMAP says "Exterior 3×3" (and "3×3 grids from FNV WastelandNV"), but `--grid 0,0 --radius 3` logs `Found 49/49 cells in 7x7 grid`. Either the ROADMAP wording is wrong or the radius semantics are wrong.

**Fix**: change "3×3" to "7×7 (radius 3)" in ROADMAP.md, OR rename the flag to `--side 3` if the intent was radius=1. The number that ships in logs is 7×7, so update the docs to match.

## FNV-D5-02: stats.texture_count is registry-wide, not cell-scoped

**Location**: [byroredux/src/commands.rs:51](byroredux/src/commands.rs#L51) + [crates/core/src/ecs/resources.rs:86](crates/core/src/ecs/resources.rs#L86).

`stats` reports global TextureRegistry size, not "textures referenced by entities in the current scene". For a single-cell session this is fine; at M40 (world streaming) the value will not drop when cells unload, masking texture-leak regressions like FNV-D3-01 / FNV-D3-02 (#626 / #627).

**Fix**: expose two fields — `texture_count` (registry, unchanged) and `textures_in_use` (count of unique handles reachable from `TextureHandle` query). Update `StatsCommand::execute`. ~10 lines.

## FNV-D5-03: --cmd headless mode has no scene access

**Location**: [byroredux/src/main.rs:84-101](byroredux/src/main.rs#L84-L101).

The `--cmd` short-circuit creates an empty `World` with only `DebugStats::default()` and a registry. So `--cmd "stats"` always returns FPS 0, entities 0, draws 0, even with `--esm`/`--bsa` flags also passed. Blocks CI regression checks against entity counts without X11.

**Fix**: either (a) reject `--cmd` if `--esm`/`--bsa` is also present (clear error: "use a live engine session for cell-aware stats"), or (b) wire the cell loader into the `--cmd` path so it can answer `entities` against a loaded cell without a window. (a) is ~3 lines; (b) is the real fix and unblocks CI baseline checks.

## FNV-D5-04: 29 unresolved REFR bases in Prospector

**Evidence**: `dump_cell_refs` reports 432/461 resolved, 29 unknown. Live bench logs `5 base forms not found in statics table (sample: 0015D9B9, 0010521D, 0016AD05, 000AE8B3, 000C79EC)` (cell_loader.rs:1223-area).

The two numbers disagree (29 vs 5) — `dump_cell_refs` counts placements, the spawn-side filter dedupes. Both nonzero means either:
- (a) ESM dispatch is missing record kinds those base FormIDs land in (CONT? PROJ? IDLM?)
- (b) those forms live in plugin overrides the single-master loader doesn't follow

**Fix**: investigate the 5 unique unresolved base FormIDs against `tes4view`. If they're CONT/IDLM/MSTT, file a record-dispatch follow-up. If they're plugin references, that's a `FormIdRemap` test gap (#445).

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: For D5-02, ensure mesh_count + draw_count have the same ambiguity audit.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: D5-02: assert `textures_in_use < texture_count` after acquiring more handles than fit in scene. D5-03: `--cmd stats --esm ...` returns nonzero entity count.

_Filed from audit `docs/audits/AUDIT_FNV_2026-04-24.md`._
