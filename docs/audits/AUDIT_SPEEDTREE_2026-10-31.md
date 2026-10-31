# SpeedTree Subsystem Audit — 2026-10-31

**HEAD**: 670f81917 · **Baseline**: [AUDIT_SPEEDTREE_2026-10-27.md](AUDIT_SPEEDTREE_2026-10-27.md) (HEAD `fa6be4a6b`) · **Audited**: Dimension 3 (TREE→Billboard Wiring; cross-cut files changed), Dimension 4 (Per-Game Variants & Route Divergence; carry-over check), Dimension 6 (NIFAL Material Translation; `ImportedMaterial` / `material_translate.rs` changed) · **Unchanged since baseline (skimmed)**: Dimension 1 (Walker Byte-Accounting), Dimension 2 (Placeholder Fallback), Dimension 5 (Tag Dictionary).

Scope: `git log fa6be4a6b..HEAD` (136 commits). `crates/spt/`, `systems/billboard.rs`, `crates/core/src/ecs/components/billboard.rs`, `crates/plugin/src/esm/records/tree.rs`, `scene/nif_loader.rs` and `streaming/pre_parse.rs` have no commits in that range.

**Depth**: `deep`. Corpus re-run for all three games. **Execution**: single pass inside `/audit-suite --preset comprehensive`; no sub-agents, no engine launch, repo `target/` only, `CARGO_INCREMENTAL=0`. Scratch notes: `/tmp/audit/speedtree/dim_1.md` … `dim_6.md`.

## Commands run

| Lane | Result |
|---|---|
| `cargo test -q -p byroredux-spt` | Pass (55 unit + 3 synthetic; 4 corpus tests ignored) |
| `cargo check -p byroredux-spt --features recon --examples` | Clean |
| `cargo test -p byroredux-spt --features recon --lib` | 60/60 |
| Corpus `--release --test parse_real_spt -- --ignored --nocapture --test-threads=1` with `BYROREDUX_{FNV,FO3,OBL}_DATA` + `BYROREDUX_REQUIRE_GAME_DATA=1` | 100 % unknown-tag-clean: FNV 10/10, FO3 10/10 (1,956 entries each), Oblivion 113/113 (22,092 entries). `walker_stops_on_true_tlv_boundary`: FNV 10, FO3 10, OBL 113, SI 26 — all on the boundary, shift 0, 0 EOF stops, 0 extract/parse failures. |
| `cargo test -p byroredux --bin byroredux -- {billboard, parse_and_import_spt, vanilla_tree, tree_model, tree_icon, spt, speedtree, malformed_spt} --include-ignored` (1.96.0 toolchain, game data present) | All pass (14 / 2 / 2 / 2 / 2 / 9 / 7 / 1) |

## Delta reviewed

| Change | SpeedTree surface | Verdict |
|---|---|---|
| `nif_import_registry.rs` eviction deferral (#6114: `eviction_deferred`, `evict_over_cap`, `DEFERRED_EVICTION_HEADROOM`) | Registry that also caches `.spt` placeholders under `spt_cache_key` | Eviction logic only; `.spt` keys are shared-model entries and follow the same LRU. No key/shape change. Cost of `evict_over_cap` is owned by AUDIT_PERFORMANCE_2026-10-31 D7-01. |
| `spawn.rs` `render_geometry_can_collide` (#6459, unlit+blended VFX sheets) | Collider predicate the placeholder quad passes through | Placeholder still has `alpha_test: true` (`crates/spt/src/import/mod.rs:389`), so it is rejected as before; `material_kind` is not `NO_LIGHTING`. No collider is synthesised for the quad. |
| `references/attach.rs` (comment), `complete.rs` (`release_held_dead_actor_reconciliations`), `references/mod.rs` (`raise_actor_attach_story_event`) | Actor / reference completion | Actor-synth and per-cell completion only; `.spt` dispatch in `synth_child.rs` / `parse_and_import_spt` unchanged. |
| `crates/nif/src/import/types/material.rs` (`character_skin`, `production_text` scan), `material_translate.rs` (`SPAWNER_ROOTS` test const) | `ImportedMaterial` literal in `placeholder_billboard_mesh` | `character_skin` defaults `false` via `..Default::default()`; placeholder has no `textures\characters\` path. Foliage overrides (`metalness_override: Some(0.0)`, `roughness_override: Some(0.85)`) unchanged. |

Invariants re-checked at HEAD:
- Mesh-level `Billboard` / `SpeedTreeWind` attach: `spawn/mesh_instance.rs:1279-1286`; loose route `nif_loader.rs:1509-1511`, `:1732-1734`.
- `boot/schedule/late.rs:81-82` declares `reads::<Billboard>()` and `reads::<SpeedTreeWind>()`.
- `streaming/pre_parse.rs` `is_spt_model_path` (`:629`) → `PreflightDecision::SkipSpt` (`:730`, `:744`) intact.
- `detect_variant` still only feeds logs.

## Dedup and baseline carry-over

| Prior item | State at HEAD |
|---|---|
| SPT-2026-10-27-D4-01 (loose/console routes admit at `Actor` tier) | Published as **#6518**, still OPEN; `scene/nif_loader.rs:1233` unchanged. Existing, not re-filed. |
| #5680, #5360, #5361, #5137, #4122 | CLOSED, not regressed (gates above green). |
| Loose `--tree` route using `SptScene::default()` on `parse_spt` Err | Intended visualiser behaviour; not a finding. |

Searches: `/tmp/audit/open_issues_snapshot.txt` for `spt|speedtree|tree|billboard|admission` → only #6518 (this subsystem) plus unrelated #4113 and #6527 (admission-tier dead reader, tech-debt). `SUITE_RULES.md` § 6 re-read before finalizing: no SpeedTree / `.spt` / TREE finding from other audits.

---

## Findings

None new.

## Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 0 |

Existing and still open: #6518 (LOW). Walker, placeholder and tag dictionary are unchanged and every gate is green on real data (159/159 on the true TLV boundary, 100 % unknown-tag-clean per game, all vanilla TREE models and ICONs resolve). Registry eviction deferral, the collider predicate, actor-completion hooks and the `ImportedMaterial` additions leave the `.spt` route intact.
