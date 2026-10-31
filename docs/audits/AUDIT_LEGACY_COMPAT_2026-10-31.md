**HEAD**: 670f81917 · **Baseline**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-10-27.md` (HEAD `fa6be4a6b`) · **Audited**: Dim 2 (the #6471 fix to spawn-sequence selection, re-verified against code), Dim 1 (the #6472 fix plus an added-line scan of the 136 in-range commits) · **Unchanged since baseline (skimmed)**: Dim 1's listed core/nif coordinate paths (zero commits), Dim 3 (Pattern A grep clean; new game branches are single named decisions)

# Legacy Compatibility Audit — 2026-10-31

## Method

Scope is `fa6be4a6b..670f81917`, 136 commits.

- **Path-scoped history.** I ran `git log` over each dimension's paths.
  - Coordinate and placement paths (`crates/core/src/math`, `import/coord.rs`, `rotation.rs`, `strip.rs`, `cell_loader/euler.rs`, `camera.rs`): no commits.
  - Animation and spawn paths (`cell_loader/spawn.rs`, `object_sequences.rs`, `crates/nif/src/anim`, `crates/core/src/animation`, `anim_convert.rs`, `crates/nif/src/import`, `crates/core/src/string`): seven commits. Only `69af7947a` (#6471) touches this audit's seams; the rest are NIFAL material or skeleton work, or doc re-points.
- **Added-line scan** of the range diff for `(x, z, -y)`, `4096.0`, `from_mat3`, `EXTERIOR_CELL_UNITS`, `zup_to_yup`/`yup_to_zup`, `normalize_quat`, `from_euler`/`EulerRot` and `euler_zup_to_quat_yup`.
  - All new conversions go through `coord.rs`.
  - The only `Vec3::new(.., -y)` forms are test fixtures, including the #6472 ratchet's own fixture.
- **Tests run** (repo `target/`, `CARGO_INCREMENTAL=0`):
  - `cargo test -p byroredux-core --lib -- math::coord components::camera named_sequences`: 49 passed.
  - `cargo test -p byroredux-nif --lib -- strip rotation import::coord anim`: 160 passed.
- **Dedup.** The open-issue snapshot (98 open) has no legacy-compat, idle-sequence or Z-up swap issue. I re-read `SUITE_RULES.md` § 6; no finished audit reports a legacy-compat overlap.

No source, skill or issue was modified, and no engine was launched. I did not re-run the 10-27 vanilla-BSA sequence probe; its outputs were not on disk, and the fix is verified by code and unit test instead. No private target dir was used. Scratch notes are in `/tmp/audit/legacy-compat/dim_{1,2,3}.md`.

## Executive Summary

| Severity | NEW | Existing / cross-referenced |
|---|---:|---:|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 0 | 0 |
| LOW | 0 | 0 |

Both 10-27 findings are fixed in tree. No new findings.

| 10-27 finding | Fix | Verification |
|---|---|---|
| LC-2026-10-27-D2-01 (HIGH): spawn played the first `NiControllerSequence` | #6471, `69af7947a` | `spawn.rs` now takes `sequences.spawn_handle()`, which is `idle`, then `specialidle`, then none (bind pose). `handles.first()` is gone and the source-shape test forbids it. Every non-playing sequence still gets its sinks attached for `PlayGroup`. `named_sequences.rs::spawn_starts_the_idle_never_an_activation_clip` pins `open`-first, `forward`-first, idle-not-first, `Idle` over `SpecialIdle`, and empty. |
| LC-2026-10-27-D1-01 (LOW): scattered-name Z-up→Y-up swap invisible to the ratchet | #6472, `576f23c03` | `exterior/apply_job.rs` now builds the point with `Vec3::from(zup_to_yup_pos([world_x, world_y, mid_height + 200.0]))`. `hand_rolled_zup_swaps` gained `scattered_name_swap` with a fixture. |

## Dimension 1 — Coordinate + placement fidelity

No listed-path commits in range. The guards pass (core `math::coord` + `camera`, nif `strip`/`rotation`/`import::coord`).

The added-line scan found these call sites:
- ObScript position and angle helpers, which use `yup_to_zup_pos`, `zup_to_yup_pos` and `euler_zup_to_quat_yup`.
- The #6472 apply-job site.

All of them route through the canonical helpers. There is no new `Quat::from_mat3`, no new `normalize_quat` caller, and no new 4096 cell literal in production.

## Dimension 2 — Legacy subsystem coverage

In range:
- `69af7947a` (#6471).
- `4385d5fe8` (#6459): unlit blended VFX sheets no longer synthesize colliders. It is keyed on the canonical material, `MATERIAL_KIND_NO_LIGHTING` plus `has_alpha`, so it is a NIFAL-consistent, single-boundary decision.
- `477124477` (#6476/#6477): `character_skin` is now computed at the import boundary rather than from a pool snapshot (NIFAL scope).

The #6471 residual design question is deliberate and not a finding. A model with no `Idle` or `SpecialIdle` now spawns in its bind pose, which is the authored rest pose. Activation clips wait for `PlayGroup` or activation consumers.

`crates/core/src/string` has no commits.

## Dimension 3 — Cross-game translation-pattern spot-check

Skimmed as unchanged. There are no findings.

- **Pattern A.** The `bsver` literal grep, with comment lines dropped, finds only three test strings in `crates/nif/src/blocks/base.rs` (530, 550, 551). The one `version.rs` change is a comment re-point.
- **New `GameKind` sites in `cell_loader`.**
  - `needs_exterior_processing` gained FO4 (#6464). It is still one named decision, and the interior-start cost is already tracked as PERF-2026-10-31-D7-02.
  - `DefaultLandTexture::for_game` is table-shaped.
  - Test fixtures.
- **Pattern C.** Record-level decode changes in range (EXPL, PROJ, WTHR, equipment, media set) belong to `/audit-esm`. See ESM-2026-10-31-01 and -02 for the EXPL/PROJ per-era splits.

---

No findings to publish.
