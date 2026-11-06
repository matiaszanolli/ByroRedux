# SpeedTree Subsystem Audit — 2026-11-06

**HEAD**: 55ba81e20 · **Baseline**: [AUDIT_SPEEDTREE_2026-10-31.md](AUDIT_SPEEDTREE_2026-10-31.md) (HEAD `670f81917`) · **Audited**: Dimension 3 (TREE→Billboard Wiring; `systems/billboard.rs`, `scene/mesh_entity.rs`, `references/{import,synth_child}.rs`, `pre_parse.rs` changed), Dimension 2 (the billboard-system half only), Dimension 6 (route check; `material_translate.rs` changed) · **Unchanged since baseline (skimmed)**: Dimension 1 (Walker Byte-Accounting), Dimension 4 (Per-Game Variants), Dimension 5 (Tag Dictionary)

Scope: `git log 670f81917..HEAD` (302 commits). `crates/spt/`, `crates/plugin/src/esm/records/tree.rs`, `crates/core/src/ecs/components/billboard.rs`, `docs/smoke-tests/m-trees.sh` and `cell_loader/references/import_tests.rs` have no commits in that range. The wiring around them changed (see Delta).

**Depth**: `deep`. **Execution**: single pass, no sub-agents, no engine launch, repo `target/` only, `CARGO_INCREMENTAL=0`. The live `m-trees.sh` smoke gate was not run (needs a Vulkan device and a bench launch; belongs to `/audit-runtime`).

## Commands run

| Lane | Result |
|---|---|
| `cargo test -p byroredux-spt` | Pass (55 unit + 3 synthetic; 4 corpus tests ignored) |
| `cargo check -p byroredux-spt --features recon --examples` | Clean |
| `cargo test -p byroredux-spt --features recon --lib` | 60/60 |
| `cargo test -p byroredux-spt placeholder` | 9/9 (includes `placeholder_billboard_sets_foliage_pbr_overrides`) |
| Corpus `--release --test parse_real_spt -- --ignored --nocapture --test-threads=1`, `BYROREDUX_{FNV,FO3,OBL}_DATA` + `BYROREDUX_REQUIRE_GAME_DATA=1` | 100 % unknown-tag-clean: FNV 10/10, FO3 10/10 (1,956 entries each), Oblivion 113/113 (22,092 entries). `walker_stops_on_true_tlv_boundary`: FNV 10, FO3 10, OBL 113, SI 26, all shift 0, 0 EOF stops, 0 extract/parse failures. |
| `cargo test -q -p byroredux --bin byroredux -- {billboard, parse_and_import_spt, vanilla_tree, tree_model, tree_icon, speedtree, malformed_spt, spt_} --include-ignored` (default toolchain 1.97.1, game data present) | All pass (15 / 2 / 2 / 2 / 2 / 7 / 1 / 9) |

## Delta reviewed

| Change | SpeedTree surface | Verdict |
|---|---|---|
| `69a3b0b30` (2026-11-04) local gusts: `billboard.rs` sets `wind_active = true` and folds `local_gust` / `gusted_strength` into `apply_speedtree_wind` | Wind bend for every `SpeedTreeWind` tree | Behaviour intentional (gusts apply in calm weather). `local_gust` is finite for non-finite `direction` / `speed` (`NaN` cast to cell 0, `.max` drops `NaN`, spatial term collapses to 0), so the #3194 defence-in-depth holds. The gate it disabled left dead code and stale docs: **SPT-2026-11-06-D3-01**. |
| `ee2862cda` (#6690) `scene/mesh_entity.rs::attach_mesh_components` | Billboard and `SpeedTreeWind` attach for both spawn paths | Verified: `if let Some(raw) = mesh.billboard_mode` and `if let Some((response, stiffness)) = speedtree_wind` (`mesh_entity.rs:262-272`). Cell route feeds `cached.speedtree_wind` (`mesh_instance.rs:1231`); loose route feeds `(is_spt && mesh.billboard_mode.is_some())` (`nif_loader.rs:1400`). One `translate_material_with_provenance` call (`mesh_entity.rs:149`); no parallel `.spt` material path. |
| `62782dd3b` / `7f2d98944` `placement_replaces_model_root` + `prepare_placed_model_scene` (`references/import.rs`, `pre_parse.rs`, `synth_child.rs:712`) | NIF parse paths | `.spt` is not routed through `parse_and_import_nif` or `prepare_placed_model_scene`; the synthetic placeholder has no root transform to discard, and `streaming/pre_parse.rs` still skips `.spt` (`is_spt_model_path`). No effect. |
| `22e0f71fc` (#6703) Late-stage systems split into `boot/schedule/late/` | Scheduler access for the billboard system | The `reads::<SpeedTreeWind>()` / `reads::<Billboard>()` / `reads_resource::<WindField>()` / `reads_resource::<TotalTime>()` / `writes::<GlobalTransform>()` declaration is intact at `boot/schedule/late/camera.rs:70-79`. The skill's pointer to `late.rs` is stale: **SPT-2026-11-06-D3-02**. |
| `light_translate` / `material_flags` / `zup_yup` moves (#6712-#6717) | `synth_child.rs` light path, `material_translate.rs` | Path renames only; the `.spt` branch is untouched. |

Invariants re-checked at HEAD: `nif_import_registry.rs` `spt_cache_key` per (model path, TREE record) intact; `CachedNifImport` for `.spt` keeps `bsx_flags = 0`, `root_flags = 0`, `attach_light_offset = None` (`references/import.rs`); `placement_root_billboard` stays a dead seam for SpeedTree (the `imported.nodes.first()` mode is `None`); `import_spt_scene` unchanged, so the OBND → BNAM → MODB → 256×512 precedence and the foliage PBR overrides are unchanged.

## Dedup and baseline carry-over

| Prior item | State at HEAD |
|---|---|
| SPT-2026-10-27-D4-01 (loose/console routes admit at `Actor` tier) | **#6518**, still OPEN, not re-filed. |
| #5680, #5360, #5361, #5137, #4122, #3192, #3194 | CLOSED, not regressed (gates above green). |

Searches: `gh issue list --state all --search "speedtree OR spt OR TREE"` (200 results) for `billboard`, `wind`, `gate`, `local gust`, `late.rs` → no match for either finding below.

---

## Findings

### SPT-2026-11-06-D3-01: `billboard.rs` keeps a dead wind early-out and docs that describe the pre-`69a3b0b30` gate
- **Severity**: LOW
- **Dimension**: TREE→Billboard Wiring
- **Location**: `byroredux/src/systems/billboard.rs:11-35`, `:56-66`, `:92-101`, `:126-133`, `:159`, `:194`; test `active_weather_direction_change_rebends_stationary_speedtree` (`:458`)
- **Status**: NEW
- **Description**: `69a3b0b30` made `let wind_active = true;` unconditional so local gusts move trees in every weather. That makes the early-out `if !camera_changed && !wind_active && !wind_state_changed { return; }` unreachable. These are left behind:
  - `wind_state_changed`, `last_wind` and the per-frame `WindField` comparison feed only that dead condition.
  - `BillboardPass.wind_active` is a constant `true`, and `if let (true, Some(tree_wind)) = (pass.wind_active, tree_wind)` tests it.
  - The module doc (lines 11-17) still says that when neither pose moved "the entire billboard loop is skipped". Lines 23-27 still say "wind reopens that gate". Lines 126-133 still say the wind-only pass is the steady state because `wind` "is non-zero for most vanilla weathers".
  - The new comment (lines 56-60) says the cost is "the handful of `SpeedTreeWind` trees". That count is not measured anywhere; a TREE-dense exterior spawns one marked placeholder per `.spt` REFR in the loaded grid.
  - `active_weather_direction_change_rebends_stationary_speedtree` no longer tests change detection: the tree is re-bent every frame, so it passes with `wind_state_changed` deleted.
- **Evidence**: `billboard.rs:61` `let wind_active = true;` → `:99` `!wind_active` is always false. The test at `:458` asserts only `before != after` after a direction change, which a free-running re-bend also satisfies.
- **Impact**: No wrong output. The risk is maintenance. A later reader trusts the #1374 / #3192 prose and "re-adds" or "removes" a gate that does not exist. In calm exterior weather, and in any parked-camera interior that has marked trees, the marked set now dirties `GlobalTransform` every frame where before it stayed clean, so the incremental world-bound path (#1374) is defeated for those entities. That cost is unmeasured here.
- **Related**: #1374, #3192, `69a3b0b30`.
- **Suggested Fix**: Delete `wind_state_changed` / `last_wind` / `BillboardPass.wind_active` and the vestigial early-out, or restore a real gate if a calm-weather cost is measured. Rewrite the module doc around "camera moved → all billboards; parked → `SpeedTreeWind` set". Replace the direction-change test with one that fails if the stationary pass stops re-bending (or drop it).

### SPT-2026-11-06-D3-02: the audit-speedtree skill points the scheduler-access declaration at a split hub
- **Severity**: LOW
- **Dimension**: TREE→Billboard Wiring (audit infrastructure)
- **Location**: `.claude/commands/audit-speedtree/SKILL.md:192` (`boot/schedule/late.rs`)
- **Status**: NEW
- **Description**: #6703 (`22e0f71fc`) moved the Late-stage registrations into `boot/schedule/late/`. The billboard system's `Access` (`reads::<SpeedTreeWind>()` and the rest) now lives in `boot/schedule/late/camera.rs:70-79`. `late.rs` still exists as the hub, so `_audit-validate.sh` passes. A scoped `grep` or `git log` on `late.rs` sees only the hub, as `_audit-common.md` warns.
- **Impact**: The next run greps the wrong file and reports the declaration missing.
- **Related**: #6703.
- **Suggested Fix**: Cite `boot/schedule/late/camera.rs` in the skill (and name the hub-split trap if it recurs). Fold into the next `/audit-sync`.

---

## Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 2 |

Existing and still open: #6518 (LOW). The walker, dictionary and placeholder importer are unchanged, and every gate is green on real data (159/159 files on the true TLV boundary, 100 % unknown-tag-clean per game, all vanilla TREE models and ICONs resolve). The wiring refactors (#6690 shared attach step, #6703 schedule split, the root-replace rule) leave the `.spt` route intact. The only residue is the dead wind gate in `billboard.rs` and a stale skill pointer.

Suggest `/audit-publish docs/audits/AUDIT_SPEEDTREE_2026-11-06.md` (labels `speedtree`, `tech-debt`; D3-01 also `doc-rot`).
