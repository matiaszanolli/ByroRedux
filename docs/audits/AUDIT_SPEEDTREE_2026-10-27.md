# SpeedTree Subsystem Audit — 2026-10-27

**HEAD**: fa6be4a6b · **Baseline**: [AUDIT_SPEEDTREE_2026-10-21.md](AUDIT_SPEEDTREE_2026-10-21.md) (HEAD `4a3e89d56`) · **Audited**: Dimension 3 (TREE→Billboard Wiring; cross-cut files changed), Dimension 4 (Per-Game Variants & Route Divergence; `scene/nif_loader.rs` changed), Dimension 6 (NIFAL Material Translation; `material_translate.rs` and the `ImportedMaterial` home changed) · **Unchanged since baseline (skimmed)**: Dimension 1 (Walker Byte-Accounting), Dimension 2 (Placeholder Fallback), Dimension 5 (Tag Dictionary).

Since the baseline, `crates/spt/` has one commit, `c4975b1ab` (#5622). It adds a reason comment to a `#[allow(clippy::too_many_arguments)]` in `crates/spt/examples/spt_tail.rs` and touches no `src/` or docs.

**Depth**: `deep`. The real-data corpus was re-run for all three games.
**Execution**: single pass inside `/audit-suite --preset comprehensive`. No sub-agents, no engine launch, repo `target/` only. Scratch notes: `/tmp/audit/speedtree/dim_1.md` … `dim_6.md`.

## Commands run

| Lane | Result |
|---|---|
| `cargo test -q -p byroredux-spt` | Pass (55 unit + 3 synthetic; 4 corpus tests ignored) |
| `cargo check -p byroredux-spt --features recon --examples` | Clean |
| `cargo test -p byroredux-spt --features recon --lib` | 60/60 |
| Corpus `--release --test parse_real_spt -- --ignored --nocapture --test-threads=1` with `BYROREDUX_{FNV,FO3,OBL}_DATA` and `BYROREDUX_REQUIRE_GAME_DATA=1` | 100 % unknown-tag-clean: FNV 10/10, FO3 10/10 (1,956 entries each), Oblivion 113/113 (22,092 entries). `walker_stops_on_true_tlv_boundary`: FNV 10, FO3 10, OBL 113, SI 26. Every file stops on the boundary with shift 0. There are 0 EOF stops, 0 extract failures and 0 parse failures. |
| `cargo test -p byroredux --bin byroredux -- {billboard, parse_and_import_spt, vanilla_tree, tree_model, tree_icon, spt, speedtree, malformed_spt} --include-ignored` (1.96.0 toolchain, game data present) | All pass (billboard 14, parse_and_import_spt 2, vanilla_tree 2, tree_model 2, tree_icon 2, spt 9, speedtree 7, malformed_spt 1) |

## Delta reviewed

| Commit | SpeedTree surface | Verdict |
|---|---|---|
| `c82fcc407` (#6160 part), `c4975b1ab` (#5622) | `synth_child.rs`: logical-child and light-tail helpers (`spawn_logical_synth_child`, `finish_light_synth_child`), plus allow reasons | Code moved without a behaviour change. The `.spt` dispatch is unchanged. `is_spt` comes from the lowercased `stat.model_path` (`synth_child.rs:579`). `resolve_spt_model_path` with `has_mesh_exact` (`:596`), `spt_cache_key` (`:638`) and `extract_mesh_exact` (`:683`) still run for `.spt` only. A SCOL substitution never yields `.spt`. |
| `4d36470b5` (#6253) | `references/import.rs` NIF branch, `CachedNifImport` docs | Only the NIF path changed: it moved to `import_embedded_animations` and `object_sequences`. The `parse_and_import_spt` literal (`import.rs:452-511`) still has `embedded_clip: None`, empty `graph_sequences` / `object_sequences`, `bsx_flags: 0`, `root_flags: 0`, `attach_light_offset: None`, `projectile_offset: None`, `furniture: None` and `speedtree_wind: Some((1.0, 0.0))`. |
| `1efe39722` | `spawn/mesh_instance.rs` collision fallback → `render_geometry_can_collide` (`spawn.rs:83-90`) | The predicate still rejects `alpha_test`, so the alpha-tested placeholder quad never gets an `ArchitectureTriMesh` collider. |
| `22d6e1a80`, `ab9d46d18` | Geometry admission tiers: `mesh_instance.rs:861`, `:907` (`Prop`), `nif_loader.rs:1229-1237` (`Actor`) | On the cell route the placeholder is admitted at `Prop`, like every other placed static. The loose and console routes admit it at `Actor`. See SPT-2026-10-27-D4-01. |
| `88149ae26` (#6097), `f72083645` (#6342) | `streaming/pre_parse.rs` two-pass prefetch | Both passes go through `admit_model` (`pre_parse.rs:715`, called at `:870` and `:1000`). Its first arm is still `is_spt_model_path` → `PreflightDecision::SkipSpt`, so no `.spt` gets a negative registry entry from either pass. `is_spt_model_path` is still a byte-level suffix test (`:629-632`). |
| `178e1e697` (#6399), `4ad1bbda7` (#6396), `5559074be` (#6141) | `LightKind`→`EmitterKind` rename; dead parameters removed from `normal_alpha_spec_*`; `ImportedMaterial` moved to `crates/nif/src/import/types/material.rs` | None of these change behaviour for the placeholder. `ImportedMaterial::default()` still gives `is_pbr: false`, `from_bgsm: false`, `external_material_resolved: false` and `emissive_source: EmissiveSource::None`. The placeholder has no normal map, so the normal-alpha-spec gate is false either way. |

Invariants re-checked at HEAD:
- `crates/spt/src`, `systems/billboard.rs`, `crates/core/src/ecs/components/billboard.rs` and `crates/plugin/src/esm/records/tree.rs` have no commits since the baseline.
- The `Billboard` insert is still at mesh level (`mesh_instance.rs:1279`), with `SpeedTreeWind` at `:1285`. The loose route inserts them at `nif_loader.rs:1508-1511` and `:1731-1734`, behind `is_spt`.
- `late.rs:81-82` still declares `reads::<Billboard>()` and `reads::<SpeedTreeWind>()`. The `systems/water.rs:704` `include_str!("billboard.rs")` pin is intact. `save_io/registry_completeness_tests.rs:596` still lists `SpeedTreeWind` as rebuilt, not saved.
- `detect_variant` still only feeds debug logs (`nif_loader.rs:303`, `references/import.rs:365`), and nothing branches on it.
- Both routes reach `translate_material_with_provenance` (`nif_loader.rs:1384`, `mesh_instance.rs:1136`). The placeholder's foliage overrides are unchanged (`crates/spt/src/import/mod.rs:388-396`).
- `tree_lod.rs` changed (`ab9d46d18`, `635330d08`, `e9eab676e`), but it is Skyrim `.btt` distant LOD and owned by `/audit-exterior`. It does not touch `.spt` or `TreeRecord`.

## Dedup and baseline carry-over

| Prior item | State at HEAD |
|---|---|
| 2026-10-21 baseline | No findings. Nothing to carry over. |
| #5680, #5360, #5361, #5137, #4122 | CLOSED. Not regressed: the docstring is unchanged and the gates above are green. |
| Loose `--tree` route uses `SptScene::default()` on a `parse_spt` `Err` (`nif_loader.rs:307-310`); the cell route returns `None` and skips the REFR | Pre-existing, intended visualiser behaviour. Not new and not filed. |

Searches:
- `/tmp/audit/open_issues_snapshot.txt` (84 open), searched for `spt` / `speedtree` / `tree` / `billboard` / `wind` / `admission`: no SpeedTree issue. The only matches are #4113 and #6414, both unrelated.
- `gh issue list --state open --search "speedtree OR spt OR billboard"` returned nothing. `gh issue list --state all --search "admission class OR admission tier"` matched nothing about geometry tiers.
- `SUITE_RULES.md` § 6 was re-read just before finalizing. No SpeedTree, `.spt`, TREE or geometry-admission finding is listed. `AUDIT_PERFORMANCE_2026-10-27.md` § "Checked, no finding" says the admission tiers are all correct ("actor bodies in `scene/nif_loader.rs` are `Actor`"). D4-01 shows the shared non-actor callers that check missed.

---

## Findings

### SPT-2026-10-27-D4-01: Loose and console routes admit a `.spt` placeholder (and every non-actor loose NIF) at the `Actor` geometry tier, while the cell route admits it at `Prop`
- **Severity**: LOW
- **Dimension**: Per-Game Variants & Route Divergence
- **Location**: `byroredux/src/scene/nif_loader.rs:1226-1237` (`spawn_nif_mesh`). Shared callers: `nif_loader.rs:138`, `:158` (`load_nif_from_args`: `--mesh` / `--tree`), `byroredux/src/debug_load.rs:150`, `byroredux/src/loading_screen.rs:347`, `byroredux/src/studio_host.rs:669`. The cell route is `byroredux/src/cell_loader/spawn/mesh_instance.rs:861`, `:907`.
- **Status**: NEW (introduced by `22d6e1a80`, 2026-10-27)
- **Description**: `22d6e1a80` wraps the one `upload_scene_mesh` call in `spawn_nif_mesh` in `with_admission_class(GeometryAdmissionClass::Actor, …)`, under the comment "Actors (the player and NPC bodies) are their own admission tier". `spawn_nif_mesh` is not actor-only, though. It is the spawn tail of `load_nif_with_skeleton_from`, which every `load_nif_bytes` caller reaches: the `--tree` / `--mesh` visualiser, the console `debug_load` path (which takes any archive path, `.spt` included, because `cached_scene_lazy` routes on the label's extension), loading-screen meshes and Studio-host loads. So one SpeedTree placeholder quad is admitted at `Prop` when a TREE REFR spawns it and at `Actor` when the loose or console route spawns it. The same applies to every non-actor NIF on those routes.
- **Evidence**:
  ```rust
  // nif_loader.rs:1229 — inside spawn_nif_mesh, used by every load_nif_bytes caller
  // Actors (the player and NPC bodies) are their own admission tier:
  // above props, below the ground, ...
  let handle = match ctx.mesh_registry.with_admission_class(
      byroredux_renderer::GeometryAdmissionClass::Actor,
  ```
  `crates/renderer/src/mesh.rs:55-63`: `Actor` = "the cap less the ground's reserve"; `Prop` = "the cap less the ground's and the actors' reserves".
- **Impact**: Small. A `--tree` / `--mesh` boot uploads into an empty pool, where the tiers make no difference. Once the pool is past the `Prop` ceiling (a large exterior radius), a console-loaded or loading-screen NIF, or a console-loaded `.spt` placeholder, still lands by eating the actor reserve that the tiers keep for people. Cell-streamed props are refused at that point. The comment also misdescribes the function, and that is what led the performance audit to sign the tiers off as correct.
- **Related**: `22d6e1a80`, `ab9d46d18` (geometry admission tiers); AUDIT_PERFORMANCE_2026-10-27 "Checked, no finding: Geometry admission tiers". The owner of the tier policy is `/audit-renderer` / `/audit-performance`. It is filed here because the SpeedTree loose route is one of the divergent call paths.
- **Suggested Fix**: Pass the admission class into `load_nif_with_skeleton_from` / `spawn_nif_mesh`. The `npc_spawn` callers use `Actor`, and `load_nif_bytes` (visualiser, `debug_load`, loading screen, Studio) uses `Prop`. Also correct the comment. Either way, a `.spt` placeholder should get the same tier on both routes.

---

## Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 1 |

Walker, placeholder and tag dictionary did not change, and every gate is green on real data. The corpus is 159/159 on the true TLV boundary and 100 % unknown-tag-clean per game, and every vanilla TREE model and ICON resolves. The cross-cut refactors since the baseline leave the `.spt` route intact: synthetic-child helpers, #6253 object sequences, the collider predicate, the two-pass prefetch and the material-type split. That covers the cache keying, the mesh-level `Billboard` / `SpeedTreeWind` attach and the placeholder material contract. The one new item is a tier mislabel on the shared loose-NIF spawn path that the geometry admission tiers introduced.

Suggested: `/audit-publish docs/audits/AUDIT_SPEEDTREE_2026-10-27.md` (labels `low`, `bug`, `speedtree`, `renderer`, `performance`).
