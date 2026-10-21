# SpeedTree Subsystem Audit — 2026-10-21

**HEAD**: 4a3e89d56 · **Baseline**: [AUDIT_SPEEDTREE_2026-10-12.md](AUDIT_SPEEDTREE_2026-10-12.md) (HEAD `5d31fd637`) · **Audited**: Dimension 3 (TREE→Billboard Wiring; changed cross-cut paths) and Dimension 6 (NIFAL Material Translation; `material_translate.rs` changed) · **Unchanged since baseline (skimmed)**: Dimension 1 (Walker Byte-Accounting), Dimension 2 (Placeholder Fallback), Dimension 4 (Per-Game Variants), Dimension 5 (Tag Dictionary). Since the baseline, `crates/spt/` has one commit, `251d491ef` (#5851). It removes the unused `env_logger` dev-dependency from `crates/spt/Cargo.toml` and touches no source.

**Depth**: `deep`. The real-data corpus was re-run for all three games.
**Execution**: single pass, no sub-agents, no engine launch, repo `target/` only. Scratch notes: `/tmp/audit/speedtree/dim_1.md` … `dim_6.md`.

## Commands run

| Lane | Result |
|---|---|
| `cargo test -q -p byroredux-spt` (1.96.0) | Pass (55 unit + 3 synthetic; 4 corpus tests ignored) |
| `cargo check -p byroredux-spt --features recon --examples` | Clean |
| `cargo test -p byroredux-spt --features recon --lib` | 60/60 |
| Corpus `--release --test parse_real_spt -- --ignored --nocapture --test-threads=1` with `BYROREDUX_{FNV,FO3,OBL}_DATA`, `BYROREDUX_REQUIRE_GAME_DATA=1` | 100 % unknown-tag-clean: FNV 10/10, FO3 10/10 (1,956 entries each), Oblivion 113/113 (22,092 entries). `walker_stops_on_true_tlv_boundary`: FNV 10, FO3 10, OBL 113, SI 26. Every file stops on the boundary with shift 0, and there are 0 EOF stops, 0 extract failures and 0 parse failures. |
| `cargo test -p byroredux --bin byroredux -- {billboard, parse_and_import_spt, vanilla_tree, tree_model, tree_icon, spt, speedtree, malformed_spt} --include-ignored` (game data present) | All pass (billboard 14, parse_and_import_spt 2, vanilla_tree 2, tree_model 2, tree_icon 2, spt 9, speedtree 7, malformed_spt 1) |

## Delta reviewed

| Commit | SpeedTree surface | Verdict |
|---|---|---|
| `e712f6c6d` (#5680) | `references/import.rs:497-498` attach-light comment | Closes the baseline's only finding (SPT-2026-10-12-D3-01). The comment now says "no `AttachLight` node". Not regressed. |
| `e49533a4c` (#5795) | `nif_import_registry.rs` `scene_satellites` / `CachedNifImport::from_scene`; `references/import.rs` | The NIF sync and streaming paths now share one constructor. `parse_and_import_spt` keeps its own literal (`import.rs:466-512`), which is correct because there is no `NifScene` to extract satellites from. The synthetic defaults hold: `bsx_flags: 0`, `root_flags: 0`, `attach_light_offset: None`, `projectile_offset: None`, `furniture: None`, `speedtree_wind: Some((1.0, 0.0))`. `from_scene` hard-codes `speedtree_wind: None` and `placement_root_billboard: None`, so no NIF route can pick up the SpeedTree wind marker. |
| `6e1853d24` (refs #6114) | `NifImportRegistry` LRU victim selection | `is_cell_unique_key` matches only `\precombined\…_oc.nif`. The `.spt` keys (`spt_cache_key`, `<path>#<formid:08x>`) still fall into the plain-LRU tier. |
| `bbb4dc3d7` (#6115), `3c57697c8` (#6119), `c420c9b22` (#6121) | `references/import.rs` NIF branch, `scene/nif_loader.rs` NIF branch, `streaming/pre_parse.rs` | These change only the NIF path. `note_scene_health` is called after `parse_nif`, past the `.spt` early return (`nif_loader.rs:291-325`). The `extract_guarded` panic containment covers the prefetch. `.spt` paths still never reach that code: `is_spt_model_path` (`pre_parse.rs:636`) → `PreflightDecision::SkipSpt` (`:760`, `:776`). |
| `f81e6e722` (#5755/#5756), `c53d87679` (#6048) | `spawn/mesh_instance.rs` MSWP rows / LAND collider material | These apply only to REFRs with material swaps or to terrain. The mesh-level `Billboard` insert (`mesh_instance.rs:1279-1280`) and the `SpeedTreeWind` insert (`:1285-1286`) are unchanged. |
| `a10b5d578` (#5523) | `synth_child.rs` `model_src` SCOL substitution | `is_spt` is still computed from `stat.model_path` (`synth_child.rs:509`, `:590`), and `resolve_spt_model_path` + `extract_mesh_exact` still run for `.spt` only (`:606-607`, `:693-694`). A SCOL is never `.spt`. |
| `2266f7778` (#4256) | `material_translate.rs` `source_shader_type` | The placeholder's `ImportedMaterial.shader_type` is `Default` (0), so it forwards as 0 and nothing branches on it for the placeholder. The foliage overrides are unchanged (`crates/spt/src/import/mod.rs:385-396`): `metalness_override: Some(0.0)`, `roughness_override: Some(0.85)`, alpha-test func 6 with threshold 0.5, and two-sided. |
| `8879b67b4` (#6124), `d20a1bf92` (#6107) | `scene/nif_loader.rs` | The NPC lazy-extract and skinned-BLAS changes don't touch the `--tree` branch. The loose route still calls `parse_spt` → `import_spt_scene(SptImportParams::default())` (`nif_loader.rs:292`, `:316-319`). It inserts `SpeedTreeWind::new(1.0, 0.0)` at `:1509` and `:1732` behind `is_spt`. |
| `a44d4a20d` (#5997) | script attach `has_3d` | This doesn't depend on the SpeedTree path. Real REFR spawns, including TREE placeholders, pass `has_3d = true`. |

Invariants re-checked at HEAD:
- `detect_variant` still only feeds debug logs (`nif_loader.rs:300`, cell route log), and nothing branches on it.
- `late.rs:81-82` declares `reads::<Billboard>()` and `reads::<SpeedTreeWind>()`. `billboard.rs`, `crates/core/.../billboard.rs` and `crates/plugin/src/esm/records/tree.rs` have had no commits since the baseline.
- `systems/water.rs:695` still pins the billboard system's source text (`include_str!("billboard.rs")`). `save_io/registry_completeness_tests.rs` still names `SpeedTreeWind` as rebuilt-not-saved.
- `placement_root_billboard` is still a dead seam for SpeedTree. `import_spt_scene` leaves `nodes[0].billboard_mode = None`, which `import_tests.rs:68` pins, so the `spawn.rs:994` consumer can't be reached from `.spt`. This was #3533 (closed). It is not a finding.

## Dedup and baseline carry-over

| Prior item | State at HEAD |
|---|---|
| SPT-2026-10-12-D3-01 / #5680 | CLOSED, fixed by `e712f6c6d`. Verified at `import.rs:497-498`. |
| #5360, #5361 | CLOSED. Not regressed (gate output above; `parser.rs` docstring unchanged). |
| #5324 / #6173 (skill text naming closed issues) | Tech-debt-owned skill text, not code. Not re-filed. |
| REFR.XSED SpeedTree seed (ESM census) | No consumer. Same reasoning as the baseline: the placeholder has nothing per-instance for a seed to vary (#3808). Not a finding. |

Searches:
- `/tmp/audit/issues.json` (119 open) for `spt`/`speedtree`/`tree`/`billboard`/`wind`/`xsed`: the only matches are skill/doc tech-debt items (#6173, #6165) and unrelated `tree` hits (#5665, #4113).
- `gh issue list --state all --search "speedtree OR spt OR billboard"`: every SpeedTree issue is closed, and none has regressed.
- `/tmp/audit/SUITE_FINDINGS.md` (read twice, the second time just before finalizing): no SpeedTree / `.spt` / TREE entry.

---

## Findings

None.

None of the SpeedTree surfaces changed behaviour between `5d31fd637` and `4a3e89d56`. The baseline's only finding (#5680) is fixed. The cross-cut refactors (shared `CachedNifImport::from_scene`, LRU eviction tiering, the pre-parse panic guard, MSWP and `source_shader_type`) leave the `.spt` route, the cache keying, the mesh-level `Billboard` / `SpeedTreeWind` attach and the placeholder material contract intact. All lanes pass with real data: corpus 159/159 on the true TLV boundary, 100 % unknown-tag-clean per game, and every TREE model and ICON resolves.

No `/audit-publish` needed.
