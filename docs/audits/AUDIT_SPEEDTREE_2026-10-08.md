# SpeedTree Subsystem Audit — 2026-10-08

**HEAD**: 00f580e09 · **Baseline**: [AUDIT_SPEEDTREE_2026-10-05.md](AUDIT_SPEEDTREE_2026-10-05.md) (HEAD `a2c24b16e`, 116 commits ago) · **Audited**: Dimension 3 (TREE→Billboard Wiring), Dimension 4 (Per-Game Variants & Route Divergence) and Dimension 6 (NIFAL Material Translation). These are the dimensions whose cross-cut paths changed. · **Unchanged since baseline (skimmed)**: Dimension 1 (Walker Byte-Accounting), Dimension 2 (Placeholder Fallback) and Dimension 5 (Tag Dictionary). No commit has touched `crates/spt/` since the baseline.

**Depth**: `deep`. The real-data corpus harness was re-run for all three games.
**Execution**: one pass in this context, with no sub-agents and no engine launch. The per-dimension notes are in `/tmp/audit/speedtree/dim_1.md` … `dim_6.md`.

## Commands run

| Lane | Result |
|---|---|
| `cargo test -p byroredux-spt` (1.96.0) | 55 unit and 3 synthetic tests pass; the 4 corpus tests are ignored |
| `cargo check -p byroredux-spt --features recon --examples` | Clean |
| `cargo test -p byroredux-spt --features recon --lib` | 60/60 |
| Corpus: `--release --test parse_real_spt -- --ignored --nocapture` with `BYROREDUX_{FNV,FO3,OBL}_DATA` and `BYROREDUX_REQUIRE_GAME_DATA=1` | 100 % unknown-tag-clean in every game: FNV 10/10, FO3 10/10, Oblivion 113/113 (22,092 entries). `walker_stops_on_true_tlv_boundary`: FNV 10, FO3 10, OBL 113 and SI 26 files all stop on the boundary with shift 0, and there are 0 EOF stops, 0 extract failures and 0 parse failures |
| `cargo test -p byroredux --bin byroredux -- {billboard, parse_and_import_spt, vanilla_tree, tree_model, tree_icon, spt, speedtree, malformed_spt} --include-ignored` (1.96.0, game data present) | All pass. `billboard` 14, `parse_and_import_spt` 2, `spt` 9, `speedtree` 7 and `malformed_spt` 1. `vanilla_tree_models_all_resolve` resolves FNV 3, FO3 9 and OBL 139 unique `.spt` MODL values. `vanilla_tree_icons_all_resolve` resolves FNV 3, FO3 9 and OBL 81 unique ICON values. |

## Delta reviewed

| Commit | SpeedTree surface | Verdict |
|---|---|---|
| `54d713dee` (#5092) splits `streaming.rs` into `streaming/{mod,pre_parse,telemetry}.rs` | The `.spt` prefetch skip | Intact. `is_spt_model_path` is now at `byroredux/src/streaming/pre_parse.rs:599`, and it still compares the raw bytes. It is consumed at `:720` → `PreflightDecision::SkipSpt`. `spt_check_tolerates_a_non_ascii_tail` passes. |
| `b24cb46b6` (#5308) pins the toolchain and rewrites clippy sites | `crates/plugin/src/esm/records/tree.rs:190` SNAM decode | `chunks_exact(4)` became `as_chunks::<4>().0.iter()`. The semantics are the same, because the trailing remainder is still dropped. SNAM is still parsed but not consumed (#3190). |
| `9f0a8a7cc` (#5230) adds the mirror-pane provenance | Both spawn routes (`spawn/mesh_instance.rs`, `scene/nif_loader.rs`) now call `translate_material_with_provenance` | The placeholder cannot reach this branch. `is_mirror_pane` needs a mesh name containing "mirror", but the placeholder mesh is named `SptPlaceholderBillboard`, and the placeholder also has `has_alpha` false. `resolve_unresolved_gloss_neutral_roughness` is gated on `bgsm_pbr_scalars_authored`, which is always false here. Both routes still pass through the single `translate_material*` boundary. |
| `46f59e1d2` (#5252) | `material_translate.rs` doc | Documentation only. No placeholder path changed. |
| `b7987d813` (#5222), `d4e8c31be` (#5248), `2d47bf7f1` (#5304) | Cell-loader neighbours: legacy LOD index, corpse set in `references/mod.rs` | None reach the `.spt` route. The new `TextureProvider::all_archive_names` sits beside `extract_mesh_exact` without changing it. |

Invariants re-checked at HEAD:
- The mesh-level `Billboard` insert is at `spawn/mesh_instance.rs:1252`, and `SpeedTreeWind` is inserted from `cached.speedtree_wind` at `:1258`.
- Both routes call `parse_spt` and then `import_spt_scene`: `references/import.rs:464/558`, and `nif_loader.rs:268/292` with `SptImportParams::default()`.
- `detect_variant` only logs, at `import.rs:474` and `nif_loader.rs:276`.
- `late.rs:81` declares `reads::<SpeedTreeWind>()`.
- `SpeedTreeWind` is still on the save allowlist (`registry_completeness_tests.rs:535`).

## Dedup and baseline carry-over

| Prior item | State at HEAD |
|---|---|
| SPT-2026-10-05-D3-01 → **#5360** (OPEN). The `m-trees.sh` attach check misreads `(no entities)`. | Still present at `docs/smoke-tests/m-trees.sh:128`. Nothing has changed in `m-trees.sh` or `tools/byro-dbg` since the baseline. **Existing: #5360.** |
| SPT-2026-10-05-D1-01 → **#5361** (OPEN). `parse_spt` says all five fatal errors are `InvalidData`, and the gate docstring names two culprits instead of three. | Still present at `crates/spt/src/parser.rs:82` and `crates/spt/tests/parse_real_spt.rs:265`. **Existing: #5361.** |
| **#5324** (OPEN, owned by tech-debt). This SKILL still tracks the open question on closed #3740. | It affects the skill text, not code. Not re-filed. |

Searches:
- Open issues (`/tmp/audit/issues.json`): `spt`, `speedtree`, `tree`, `billboard`, `m-trees`, `no entities`, `wind`. They match only #5360, #5361, #5324 and unrelated items (#4913, #5367, #4113).
- `gh issue list --state all --search "speedtree OR spt OR TREE OR m-trees"`: no regression of any closed SpeedTree issue was found.

---

## Findings

None new.

Two findings are already tracked and still reproduce at HEAD. They are carried forward and not re-filed:
- **#5360**: LOW, TREE→Billboard Wiring. The attach check in `docs/smoke-tests/m-trees.sh:126-131` does not match byro-dbg's `byro> (no entities)` line, so a real zero-billboard regression is diagnosed as an attach failure.
- **#5361**: LOW, Walker Byte-Accounting / docs. The docs are stale in two places:
  - `crates/spt/src/parser.rs:82-90` says underflow returns `InvalidData`, but it returns `UnexpectedEof`.
  - `crates/spt/tests/parse_real_spt.rs:263-266` names two #4122 culprits; there are three, including `10003`.

---

## Dimension summary

| Dimension | Delta since 10-05 | New findings | Notes |
|---|---|---:|---|
| 1: Walker Byte-Accounting | none | 0 | Skimmed. The corpus is 100 % clean and on the boundary (159/159). #5361 is still open. |
| 2: Placeholder Fallback | none | 0 | Skimmed. The placeholder and billboard-system tests pass. |
| 3: TREE→Billboard Wiring | streaming split, SNAM `as_chunks`, provenance call swap | 0 | The prefetch skip survived the split. The real-archive MODL and ICON resolve gates are green. #5360 is still open. |
| 4: Per-Game Variants & Route Divergence | `nif_loader.rs` provenance call swap | 0 | Both routes stay in parity. `detect_variant` is still log-only. |
| 5: Tag Dictionary | none | 0 | Skimmed. 0 unknown tags in the corpus. |
| 6: NIFAL Material Translation | #5230 provenance, #5252 doc | 0 | The placeholder cannot reach the new mirror-pane exemption. The foliage overrides and alpha-test cutout are unchanged. |

**Totals**: 0 new findings (0 CRITICAL, 0 HIGH, 0 MEDIUM, 0 LOW). 2 existing findings are still open: #5360 and #5361, both LOW.

## Summary

No SpeedTree production code changed in this window. `crates/spt/` was untouched. The cross-cut edits were the `streaming.rs` split, the `tree.rs` clippy rewrite and the #5230 material-provenance call swap. None of them changed `.spt` behaviour:
- The char-safe `.spt` prefetch skip moved intact into `streaming/pre_parse.rs`.
- The SNAM decode is semantically identical.
- The placeholder cannot reach the new mirror-pane exemption.

The walker holds its measured best state in all three games, and both real-archive resolve gates pass. The two LOW findings from the baseline (#5360 and #5361) are open and still reproduce.

### Suggested next step

Nothing new to publish. Fixing #5360 and #5361 closes the subsystem's open backlog.
