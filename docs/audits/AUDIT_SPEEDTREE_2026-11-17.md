# SpeedTree Subsystem Audit — 2026-11-17

**HEAD**: d1e7b0f81 · **Baseline**: [AUDIT_SPEEDTREE_2026-11-06.md](AUDIT_SPEEDTREE_2026-11-06.md) (HEAD `55ba81e20`) · **Audited**: all six dimensions (Walker Byte-Accounting, Placeholder Fallback, TREE→Billboard Wiring, Per-Game Variants, Tag Dictionary, NIFAL Material Translation)

Scope: `git log 55ba81e20..HEAD` (641 commits). `crates/spt/` changed once (`02453c0c3`, #6988, deleted `SptScene::count_by_kind`, an unreferenced item). `crates/plugin/src/esm/records/tree.rs`, `crates/core/src/ecs/components/billboard.rs`, `docs/smoke-tests/m-trees.sh` and `cell_loader/references/import_tests.rs` have no commits in the range. The SpeedTree-facing changes are the three listed under *Delta reviewed*.

**Depth**: `deep`. **Execution**: single pass, no sub-agents, no engine launch, `CARGO_INCREMENTAL=0`. The live `m-trees.sh` gate was not run (needs a Vulkan device and a bench launch; it belongs to `/audit-runtime`).

## Commands run

| Lane | Result |
|---|---|
| `cargo test -p byroredux-spt` | Pass (55 unit + 3 synthetic; 4 corpus tests ignored) |
| `cargo check -p byroredux-spt --features recon --examples` | Clean |
| `cargo test -p byroredux-spt --features recon --lib` | 60/60 |
| Corpus `--release --test parse_real_spt -- --ignored --nocapture --test-threads=1`, `BYROREDUX_{FNV,FO3,OBL}_DATA` + `BYROREDUX_REQUIRE_GAME_DATA=1` | 100 % unknown-tag-clean: FNV 10/10, FO3 10/10 (1,956 entries each), Oblivion 113/113 (22,092 entries). `walker_stops_on_true_tlv_boundary`: FNV 10, FO3 10, Oblivion 113, Shivering Isles 26 — every file on the boundary, shift 0, 0 extract-fail, 0 parse-fail |
| `cargo test -q -p byroredux --bin byroredux -- {billboard, parse_and_import_spt, vanilla_tree, tree_model, tree_icon, speedtree, malformed_spt, spt_, placeholder} --include-ignored` (rustc 1.100.0, game data present) | All pass (19 / 2 / 2 / 2 / 2 / 7 / 1 / 10 / 8) |

## Delta reviewed

| Change | SpeedTree surface | Verdict |
|---|---|---|
| `27796ed66` (#6722) `systems/billboard.rs` | The wind early-out and its stale docs, prior finding SPT-2026-11-06-D3-01 | **Fixed.** `wind_state_changed`, `last_wind` and `BillboardPass.wind_active` are gone. The camera-moved branch walks every `Billboard`; the parked branch walks the `SpeedTreeWind` set and intersects it with `Billboard`. The module doc now states the marked trees are re-bent every frame, calm weather included. The tests were rewritten to pin it: `parked_camera_in_calm_weather_still_rebends_marked_tree_only`, `parked_camera_under_wind_does_not_redirty_ordinary_billboard`, `parked_camera_wind_pass_skips_a_marked_entity_without_billboard`, `wind_direction_change_rebends_stationary_speedtree`. |
| `4796a8109` (#6518) `scene/nif_loader.rs` | Admission tier of the loose / `--tree` route, prior finding SPT-2026-10-27-D4-01 | **Fixed.** `load_nif_bytes` passes `GeometryAdmissionClass::Prop` (`nif_loader.rs:205`), so a `.spt` placeholder no longer draws on the actor reserve. Only the NPC, gear and Cornell entry points pass `Actor`. Two source-shape pins cover it (`nif_loader.rs:1888-1921`). |
| `da29e5837` (#6956) `asset_provider/archive.rs`, `references/{import,synth_child}.rs`, `nif_import_registry.rs` | `resolve_spt_model_path`'s `meshes_rooted` closure, the non-`.spt` branch of `synth_child`, `canonical_model_path_key` and so `spt_cache_key` | **Behaviour-preserving for `.spt`.** `meshes_rooted` is now `normalize_mesh_path`: the same `meshes\`-prefix rule, and a leading separator is kept exactly as before (`meshes\\Name.spt`, which the resolver's next probe handles). `trees\` prefixing stays `.spt`-scoped in `SPT_CANDIDATE_DIRS`; it was not hoisted into the shared normaliser. `canonical_model_path_key` now trims leading separators, so a `.spt` cache key differs in text from before, and both the producer and every reader go through the same function. `vanilla_tree_models_all_resolve` and `vanilla_tree_icons_all_resolve` pass against the real archives. |
| `02453c0c3` (#6988) `crates/spt/src/scene.rs` | `SptScene::count_by_kind`, 32 lines | Unreferenced outside the crate; tests and corpus gates pass without it. |
| `81e8e7f23` / `607e2e9a1` / `3c9949ec9` | Parameter-struct refactor, edition-2024, toolchain pin | No SpeedTree-visible change; the spt lanes pass on 1.100.0. |

Invariants re-checked at HEAD:
- `spt_cache_key` is per (model path, TREE form id); `distinct_tree_records_sharing_one_spt_get_distinct_keys` passes.
- The streaming prefetch skips `.spt` through the byte-level `is_spt_model_path` (`pre_parse.rs:638`).
- `reads::<SpeedTreeWind>()` is declared on the billboard system (`boot/schedule/late/camera.rs:77`).
- The `--tree` route calls `parse_spt` + `import_spt_scene` with `SptImportParams::default()` and routes the mesh through the shared spawn tail (`nif_loader.rs:300-330`, `:1416`).

Dimension 1 and 5 spot checks at HEAD: `read_payload`'s sizes match `tag.rs` (`8003 | 8005 | 8009` = 52, `13008` = 11, `13013` = 4, `10002` / `10003` stride 32, `12002` = 16, `12003` = 20, `13005` bimodal); array and string caps are 64 KiB on the byte count (`parser.rs:265`, `stream.rs:98`, `:129`) with a saturating multiply; the confounder tags `4096` and `5376` stay `Unknown`. No walker, dictionary, placeholder-size or material-override change in the range.

## Dedup and baseline carry-over

| Prior item | State at HEAD |
|---|---|
| SPT-2026-11-06-D3-01 | **#6722**, CLOSED, fixed in `27796ed66`; the fix verified above. |
| SPT-2026-10-27-D4-01 | **#6518**, CLOSED, fixed in `4796a8109`; the fix verified above. |
| #5680, #5360, #5361, #5137, #4122, #3194 | CLOSED, not regressed (gates above green). |

`gh issue list --state all --search "speedtree OR spt OR TREE"` returned 200 results; none of the OPEN ones concerns `crates/spt`, the TREE→billboard wiring or the placeholder material.

---

## Findings

No new findings.

Carried observation, not a defect: the per-frame cost of re-bending every `SpeedTreeWind` placeholder in a parked-camera exterior is still unmeasured. The code says so itself (`billboard.rs`, "its per-frame cost has not been measured, and a gate should come back only with a measurement behind it"). A measurement belongs to `/audit-performance`, not here.

Not covered by this audit: the live `m-trees.sh [fnv|fo3|obl|all]` entity-and-`Billboard` floors (needs a Vulkan device; `/audit-runtime`), and the open question whether Oblivion should size its placeholder from BNAM or MODB, which still needs a source rather than a re-file.
