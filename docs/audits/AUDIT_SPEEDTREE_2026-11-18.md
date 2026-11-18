# SpeedTree Subsystem Audit — 2026-11-18

**HEAD**: f8e0bac1a · **Baseline**: [AUDIT_SPEEDTREE_2026-11-17.md](AUDIT_SPEEDTREE_2026-11-17.md) (HEAD `d1e7b0f81`) · **Audited**: all six dimensions (Walker Byte-Accounting, Placeholder Fallback, TREE→Billboard Wiring, Per-Game Variants, Tag Dictionary, NIFAL Material Translation)

Scope: `git log d1e7b0f81..HEAD` (29 commits, one day after the baseline). `crates/spt/` has no diff in the range. `byroredux/src/systems/billboard.rs`, `crates/core/src/ecs/components/billboard.rs`, `crates/plugin/src/esm/records/tree.rs`, `cell_loader/references/import.rs`, `cell_loader/nif_import_registry.rs`, `cell_loader/spawn/mesh_instance.rs`, `scene/mesh_entity.rs`, `scene/nif_loader.rs`, `streaming/pre_parse.rs`, `material_translate.rs` and `docs/smoke-tests/m-trees.sh` have no commits in the range. One commit touches the SpeedTree wiring directory, and it does not change the `.spt` route (see *Delta reviewed*).

**Depth**: `deep`. **Execution**: single pass, no sub-agents, no engine launch, `CARGO_INCREMENTAL=0`, rustc 1.100.0. The live `m-trees.sh` gate was not run (needs a Vulkan device and a bench launch; it belongs to `/audit-runtime`).

## Commands run

| Lane | Result |
|---|---|
| `cargo test -p byroredux-spt` | Pass (55 unit + 3 synthetic; 4 corpus tests ignored) |
| `cargo check -p byroredux-spt --features recon --examples` | Clean |
| `cargo test -p byroredux-spt --features recon --lib` | 60/60 |
| Corpus `--release --test parse_real_spt -- --ignored --nocapture --test-threads=1`, `BYROREDUX_{FNV,FO3,OBL}_DATA` + `BYROREDUX_REQUIRE_GAME_DATA=1` | 4/4. Unknown-tag-clean 100.00 %: FNV 10/10 and FO3 10/10 (1,956 entries each), Oblivion 113/113 (22,092 entries). `walker_stops_on_true_tlv_boundary`: FNV 10, FO3 10, Oblivion 113, Shivering Isles 26, every file on the boundary with shift 0, 0 eof, 0 extract-fail, 0 parse-fail |
| `cargo test -q -p byroredux --bin byroredux -- {billboard, parse_and_import_spt, vanilla_tree, tree_model, tree_icon, speedtree, malformed_spt, spt_, placeholder} --include-ignored` (game data present) | All pass (19 / 2 / 2 / 2 / 2 / 7 / 1 / 10 / 8) |

## Delta reviewed

| Change | SpeedTree surface | Verdict |
|---|---|---|
| `00c2bf882` `cell_loader/references/synth_child.rs` (+15 lines, end of `spawn_synth_child`) | The shared synth-child spawn tail that the `.spt` route also passes through | **No effect on `.spt`.** The new block builds a `TriggerVolume` from `cached.phantom_bounds`, and `parse_and_import_spt` builds its `CachedNifImport` with `phantom_bounds: None` (`import.rs:537`), so the condition `let Some(bounds) = cached.phantom_bounds` is never met for a TREE placement. It is also gated on the script having a trigger block. No `.spt` test moved. |
| Everything else in the range | Skill-use leveling, pathgrid travel, new-game routes, renderer shadow pass, save and CI fixes | None of it touches `crates/spt`, the TREE→billboard wiring, the placeholder material or `.spt` path resolution. |

Invariants re-checked at HEAD, by the guards above rather than a fresh source read, since none of the guarded files changed since the 2026-11-17 audit read them:
- `spt_cache_key` is per (model path, TREE form id): `parse_and_import_spt` and the `distinct_tree_records_sharing_one_spt_get_distinct_keys` guard pass.
- The streaming prefetch still skips `.spt` through the byte-level `is_spt_model_path`.
- The `.spt` model and icon resolvers still find every vanilla record: `vanilla_tree_models_all_resolve` and `vanilla_tree_icons_all_resolve` pass against the real archives.
- The billboard system's wind behaviour holds: all 19 `billboard` tests pass, including the four parked-camera pins added by #6722.
- Walker byte accounting holds: no `crates/spt` diff, the boundary gate is 159/159, and the synthetic pinned-bytes fixtures pass.
- Placeholder material and sizing guards (`placeholder`, `speedtree`, `malformed_spt`) pass.

## Dedup and baseline carry-over

| Prior item | State at HEAD |
|---|---|
| SPT-2026-11-06-D3-01 | **#6722**, CLOSED, fixed in `27796ed66`; still pinned by the `billboard` tests above. |
| SPT-2026-10-27-D4-01 | **#6518**, CLOSED, fixed in `4796a8109`; no change to `nif_loader.rs` since. |
| #5680, #5360, #5361, #5137, #4122, #3194 | CLOSED, not regressed (gates above green). |

`gh issue list --state open --search "speedtree OR spt OR TREE"` returned 14 open issues; none concerns `crates/spt`, the TREE→billboard wiring or the placeholder material. They are M42 / M65 / volumetric-lighting trackers, an FO4 corpse-pose issue and Starfield issues that matched on "TREE" or "SPT" in other words.

---

## Findings

No new findings.

Carried observations, not defects:
- The per-frame cost of re-bending every `SpeedTreeWind` placeholder in a parked-camera exterior is still unmeasured. The code says so itself (`billboard.rs`: "its per-frame cost has not been measured, and a gate should come back only with a measurement behind it"). A measurement belongs to `/audit-performance`.
- Whether Oblivion should size its placeholder from BNAM or MODB still needs a source rather than a re-file.

Not covered by this audit: the live `m-trees.sh [fnv|fo3|obl|all]` entity-and-`Billboard` floors (needs a Vulkan device; `/audit-runtime`).
