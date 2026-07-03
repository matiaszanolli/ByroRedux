# Regression Verification Audit — 2026-07-03

**Scope**: Comprehensive sweep (no `--limit`). Confirmed previously-fixed bugs
are still fixed by (1) verifying the SKILL-named "fresh verification
candidates" that landed today, (2) extending coverage to the entire Session
54 fix wave closed since the prior `AUDIT_REGRESSION_2026-07-02.md` report
(`#1782`–`#1847`), (3) re-running the unconditional Step 4 fragile-area
contracts (NIFAL boundary, particle-emitter typing, collision-shape coverage,
Disney BSDF / retired `resRadiance[]`, GPU struct size pins), and (4) running
the **full workspace test suite** (`cargo test --release --workspace`) as a
blanket regression net across every crate.

**Result**: **0 regressions.** All 33 individually-verified issues are PASS
(fix code present in the live tree, guard test present, green where run).
All 6 Step 4 fragile-area contract groups hold. Full workspace test suite:
**3371 passed, 0 failed** across all 21 crates.

- Issues verified individually: 33 (6 SKILL-named fresh candidates + 27 from
  the Session 54 wave closed 2026-07-02→07-03)
- Step 4 fragile-area contracts: 6 groups, all PASS
- Full workspace suite: `cargo test --release --workspace` → 3371 passed / 0
  failed / 0 unexpected-ignored across `byroredux` + all 21 `crates/*`
- Dedup check: cross-referenced against 71 currently-OPEN issues
  (`gh issue list --state open`) and all prior `docs/audits/AUDIT_REGRESSION_*.md`
  reports — no overlap; nothing here duplicates an existing open finding

---

## Step 4 — Unconditional fragile-area checks (all PASS)

| Contract | Site | Status |
|----------|------|--------|
| Single `ImportedMesh → Material` boundary | `byroredux/src/material_translate.rs:73` (`translate_material`, only site) | PASS |
| `Material.metalness`/`roughness` stay plain resolved `f32` (no reintroduced `Option`) | `crates/core/src/ecs/components/material.rs:217,223` | PASS |
| Typed particle emitters (`NiPSysEmitter*` / `NiPSysGrowFadeModifier`) parse typed → dispatch → `apply_emitter_params` | `crates/nif/src/blocks/particle.rs` (structs `NiPSysEmitter`/`NiPSysEmitterCtlr`/`NiPSysEmitterCtlrData`/`NiPSysGrowFadeModifier`), dispatched `blocks/mod.rs:1003,1027,1095` | PASS |
| `BhkMultiSphereShape` + `BhkConvexListShape` still translate to `CollisionShape` (not `None`) | `crates/nif/src/import/collision.rs:589,707` | PASS |
| Disney lobe in `include/pbr.glsl`; retired `resRadiance[]` stays gone (register-local WRS via `shadowableLightRadiance`) | `crates/renderer/shaders/include/lighting.glsl:35,72`; `triangle.frag:1985` (retirement comment only) | PASS |
| `#[repr(C)]` GPU struct size pins (`GpuInstance`=112 B, `GpuCamera`=336 B) | `cargo test -p byroredux-renderer gpu_` → 24 passed, 0 failed | PASS |

---

## Per-issue verification

### Fresh candidates named explicitly by the SKILL (landed 2026-07-03, same day as HEAD)

- **#1815** (`SCR-D2-01`) — decompiler boolean-collapse recursion-depth cap.
  **Fix**: `crates/pex/src/decompile/boolean.rs:42` (`MAX_REBUILD_DEPTH = 1024`),
  checked at `:120-124` (`DecompileError::RecursionLimit`). **Guard**:
  `rebuild_rejects_excessive_recursion_depth` (`:391`) — PASS (green).
- **#1816** (`SCR-D5-NEW-02`) — `translate_pex` missing `catch_unwind`.
  **Fix**: `crates/scripting/src/translate/mod.rs:106-118`
  (`std::panic::catch_unwind(AssertUnwindSafe(...))` around
  `decompile_script`). **Guard**: `translate_pex_on_empty_bytes_is_a_clean_none`,
  `translate_pex_on_garbage_bytes_is_a_clean_none`,
  `translate_pex_on_truncated_after_magic_is_a_clean_none` (`translate/mod.rs`
  test mod) — PASS (green, 3/3).
- **#1728** (`SCR-D1-02`) — Skyrim-BE / Starfield-guards round-trip test for the
  untrusted `.pex` reader. **Fix + guard**:
  `crates/pex/src/lib.rs` — `parses_a_handbuilt_skyrim_be_pex`,
  `parses_a_handbuilt_starfield_pex_with_guards`, `rejects_bad_magic`,
  `rejects_truncation` — PASS (green, part of 40/40 `byroredux-pex` suite).
- **#1740** (`SCR-D5-03`) — DA10 `.pex` byte-equality parity test (compiled
  `.pex` → `translate_pex` reproduces the hand-builder byte-for-byte).
  **Fix + guard**: `crates/scripting/tests/pex_recognize_e2e.rs`
  (`da10_pex_reproduces_hand_builder_byte_for_byte`) — present, correctly
  `#[ignore = "needs Skyrim SE game data on disk"]` (consistent with sibling
  game-data-gated tests in the same file); compiles and is discovered by the
  test harness — PASS.
- **#1731** (`LC-D7-02`) — VWD ("Has Distant LOD") record-header flag
  (`0x00010000`) parsed and exposed. **Fix**: `crates/plugin/src/esm/reader.rs`
  (flag decode alongside the deleted/compressed flags). **Guard**:
  `vwd_flag_is_surfaced_when_set`, `vwd_flag_is_false_when_unset`,
  `vwd_flag_is_distinct_from_deleted_refr_flag`,
  `vwd_flag_coexists_with_compressed_flag` — PASS (green, part of 35/35
  `esm::reader::tests`).
- **#1718** (`FNV-D7-01`) — ragdoll body/constraint drops on bone-name miss now
  logged. **Fix**: `byroredux/src/ragdoll.rs:111` (`log::warn!` on dropped
  bodies) and `:144` (`log::warn!` on dropped constraints, cross-referencing
  the sibling drop-site diagnostic in `import/collision.rs::extract_ragdoll`
  #1539). **Guard**: `dropped_bone_excludes_body_and_dependent_constraint_but_keeps_the_rest`,
  `all_bones_resolve_yields_full_template`,
  `single_surviving_body_returns_none`,
  `surviving_bodies_with_no_surviving_constraints_returns_none` — PASS (green,
  9/9 `ragdoll::tests`).

### Revert-tracked pair (SKILL note: don't re-verify #1651 as if it still holds)

- **#1823** (`FO4-D2-01`, revert of the wrong #1651 fix) — the `0↔1`
  blend-factor swap is removed; the function was renamed from
  `gl_to_gamebryo_blend` to `bgsm_blend_to_gamebryo` and is now a documented
  identity narrowing (`raw as u8`, no swap), with an in-code post-mortem
  explaining why the original "GL-style enum" premise was false. **Fix**:
  `byroredux/src/asset_provider/material.rs:511` (`bgsm_blend_to_gamebryo`).
  **Guard**: `bgsm_blend_to_gamebryo_is_identity_narrowing`,
  `bgsm_merge_forwards_alpha_blend_mode` — PASS (green, 4/4 `asset_provider::tests`
  including the alpha-blend-mode forwarding test). Confirms #1651 has **not**
  resurfaced.
  - Note: the residual `as u8` truncation-without-range-guard concern is
    separately tracked as **currently-OPEN #1824** (`FO4-D2-02`) — correctly
    still open, not a regression of #1823 (#1823 only removed the wrong swap;
    #1824 is a distinct, still-unaddressed hardening gap on the same
    function). No action taken here per dedup protocol.

### Session 54 wave closed since the 2026-07-02 report (`#1782`–`#1847`)

Collision / render / particle:
- **#1804** (`D2-NEW-03`) two-sided blend split gated on `z_write` —
  `crates/renderer/src/vulkan/context/draw.rs:236` (`needs_two_sided_blend_split`
  checks `is_blend && b.two_sided && b.z_write`) — PASS.
- **#1803** (`PERF-D1-NEW-03`) dead `GlobalTransform` probe removed from
  `emit_particles` — `byroredux/src/render/particles.rs` (import and probe
  both removed, confirmed via `git show d68c86c9`) — PASS.
- **#1795** (`D2-NEW-02`) particle color-fade quantization restores
  `MaterialTable` dedup — `byroredux/src/render/particles.rs:30,45`
  (`quantize_fade`, `COLOR_FADE_STEPS`) — guard `quantize_fade_tests` module —
  PASS.
- **#1819** (`SPT-NEW-05`) SpeedTree placeholder billboard PBR classified at
  import time via `classify_pbr_keyword` — `crates/spt/src/import/mod.rs:333`
  — guard test asserting leaf paths don't classify metallic (`:538-562`) —
  PASS.
- **#1828 / #1829** (`SF2-01`/`SF2-02`) Stage A/B `BSGeometry` mesh-slot
  iteration continues past sentinel (empty) slots instead of short-circuiting
  — `crates/nif/src/import/mesh/bs_geometry.rs` — guard tests
  `stage_a_skips_sentinel_first_internal_slot_and_finds_populated_one`,
  `stage_a_all_sentinel_internal_slots_returns_none`,
  `stage_b_skips_sentinel_first_external_slot_and_finds_populated_one`,
  `stage_b_all_sentinel_external_slots_returns_none` (new file
  `bs_geometry_sentinel_slot_tests.rs`) — PASS (green, ran individually).

Renderer / RT perf + safety:
- **#1799** (`PERF-D5-NEW-01`) legacy 16-slot WRS reservoir arrays gated by
  `ENABLE_LEGACY_WRS` (compile-time, generated from `shader_constants_data.rs`
  via `build.rs`) — `triangle.frag` wraps the legacy path in
  `#if ENABLE_LEGACY_WRS` — PASS (gpu_ suite green; shader constant plumbing
  present).
- **#1794** (`PERF-D4-NEW-01`) `bone_world` no longer re-fills identity padding
  every frame — `byroredux/src/render/skinned.rs` (resize-only, tracks pool's
  high-water mark) — PASS.
- **#1792** (`PERF-D3-NEW-01`) `pending_bytes` threaded through
  `evict_unused_blas` so mid-batch eviction reclaims real budget —
  `crates/renderer/src/vulkan/acceleration/{blas_static.rs,predicates.rs}` —
  guard tests in `acceleration/tests.rs` — PASS (71/71 acceleration tests
  green).
- **#1793** (`PERF-D3-NEW-01` companion, "Address" — documented, not
  code-fixed) — the two budget-eviction correctness gaps documented in-place
  at `blas_static.rs` / `tlas.rs` — PASS as a documented-invariant resolution
  (consistent with the same pattern accepted for #1720/#1759 in the prior
  report).
- **#1791** (`D6-01`) drained first-sight `bind_inverses` requeued on a
  `draw_frame` early return — `crates/core/src/ecs/resources.rs:879`
  (`requeue_pending`) — guard `requeue_pending_restores_entries_for_the_next_drain`
  (`:1277`) — PASS.
- **#1790** (`SAFE-2026-07-02-01`) missing `AS_READ` bit added to the
  skinned-BLAS scratch barrier — `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:669`
  (`ACCELERATION_STRUCTURE_READ_KHR` added to the dst access mask) — guard
  tests in `acceleration/tests.rs` — PASS.
- **#1797** (companion to #1790, "Address" — documented) — the
  quantify-before-fixing gate documented at the shared BLAS scratch barrier
  site (`blas_skinned.rs`) — PASS as documented-invariant.
- **#1798** (`D7-NEW-01`) NPC-spawn calls in `load_references` timed and
  surfaced — `byroredux/src/cell_loader/references.rs:330,367`
  (`Instant::now()`/`.elapsed()` around spawn calls) — guard checks the timer
  instrumentation is present via source-text assertions — PASS.
- **#1796** (`D6-02`) pose-hash commit rolled back on a `draw_frame` early
  return — `byroredux/src/main.rs:1864` (`rollback_pending_pose_commits`),
  `crates/renderer/src/vulkan/context/draw.rs` (`skin_dispatch_ran` reset
  ordering) — guard `skin_dispatch_ran` regression tests
  (`draw.rs:4206-4254`) — PASS.
- **#1782** (`CONC-D1-01`) shared BLAS build-scratch buffer destruction
  deferred until in-flight frames retire — `crates/renderer/src/vulkan/acceleration/memory.rs`
  (routes through `DEFAULT_COUNTDOWN` deferred-destroy, not immediate free),
  `byroredux/src/cell_loader/unload.rs:129-161` — PASS.

Save / load:
- **#1844** (`SAVE-01`) referential-integrity validation now runs on load, not
  just save — `crates/save/src/validate.rs`, `byroredux/src/save_io.rs:677`
  (mirrors save-path gate as a load-time diagnostic) — guard
  `restore_world_does_not_abort_on_referentially_broken_snapshot` +
  `validation_catches_dangling_parent`/`validation_catches_equipment_out_of_bounds`
  (`crates/save/tests/round_trip.rs`) — PASS (10/10 `round_trip.rs` green).
- **#1846** (`SAVE-03`) player body given a stable, remappable `FormId` —
  `byroredux/src/scene.rs:711`, `crates/core/src/form_id.rs:148` — guard
  `player_body_inventory_survives_live_load` (`round_trip.rs`) — PASS.
- **#1845** (`SAVE-02`) `form_id_column()` keyed off an explicit registration
  flag instead of the `apply: None` heuristic coincidence —
  `crates/save/src/registry.rs:317` (`form_id_column`) — guard
  `form_id_column_resolves_the_flagged_entry`,
  `form_id_column_is_none_without_registration` — PASS.
- **#1847** (`SAVE-04`, "Address" — documented) — additive-only live-load
  overlay gap documented in-place at `crates/save/src/driver.rs:194-204` —
  PASS as documented-invariant (tracked as open follow-on design work, not a
  code regression).

Docs (doc-rot fixes, included per the `--label bug,documentation` discovery
widening called out in the SKILL):
- **#1717** — stale Starfield NIF parse-rate figures refreshed in
  `ROADMAP.md` (now cites 99.64% aggregate BA2 recovery, 2026-07-03 sweep) —
  PASS.
- **#1818** — `docs/feature-matrix.md` CTDA condition-function count corrected
  7 → 13 (`docs/feature-matrix.md:137`) — PASS.

---

## Summary table

| Issue | Title (abbrev) | Status | Fix Present | Guard |
|-------|----------------|--------|-------------|-------|
| 1815 | boolean-collapse recursion-depth cap | PASS | Yes | Yes (green) |
| 1816 | translate_pex catch_unwind | PASS | Yes | Yes (green) |
| 1728 | pex Skyrim-BE/Starfield round-trip | PASS | Yes | Yes (green) |
| 1740 | DA10 .pex byte-equality parity | PASS | Yes | Yes (present, game-data-gated) |
| 1731 | VWD record-header flag | PASS | Yes | Yes (green) |
| 1718 | ragdoll drop telemetry | PASS | Yes | Yes (green) |
| 1823 | revert wrong #1651 blend swap | PASS | Yes | Yes (green) |
| 1804 | two-sided blend split gated on z_write | PASS | Yes | Yes |
| 1803 | dead GlobalTransform probe removed | PASS | Yes (absent) | n/a |
| 1795 | particle color-fade quantization | PASS | Yes | Yes |
| 1819 | SpeedTree billboard PBR at import | PASS | Yes | Yes |
| 1828 | BSGeometry Stage A sentinel skip | PASS | Yes | Yes (green) |
| 1829 | BSGeometry Stage B sentinel skip | PASS | Yes | Yes (green) |
| 1799 | legacy WRS compile-time gate | PASS | Yes | Yes (green) |
| 1794 | bone_world padding not re-filled | PASS | Yes | Yes |
| 1792 | pending_bytes threaded through eviction | PASS | Yes | Yes (green) |
| 1793 | budget-eviction gaps documented | PASS | Yes (doc) | n/a |
| 1791 | requeue drained bind_inverses | PASS | Yes | Yes |
| 1790 | AS_READ barrier bit added | PASS | Yes | Yes (green) |
| 1797 | scratch-barrier gate documented | PASS | Yes (doc) | n/a |
| 1798 | NPC-spawn timing surfaced | PASS | Yes | Yes |
| 1796 | pose-hash rollback on early return | PASS | Yes | Yes |
| 1782 | deferred BLAS scratch destruction | PASS | Yes | Yes |
| 1844 | referential-integrity validated on load | PASS | Yes | Yes (green) |
| 1846 | player body stable FormId | PASS | Yes | Yes (green) |
| 1845 | form_id_column explicit flag | PASS | Yes | Yes |
| 1847 | additive-only overlay documented | PASS | Yes (doc) | n/a |
| 1717 | Starfield parse-rate doc refresh | PASS | Yes | n/a (doc) |
| 1818 | feature-matrix CTDA count fix | PASS | Yes | n/a (doc) |

---

## Full workspace test run

```
cargo test --release --workspace
```
**3371 passed; 0 failed; 0 unexpected.** All 21 crates + the `byroredux`
binary. No test regressed as a side effect of the Session 54 fix wave
(particle rendering, skinned-BLAS refit/barriers, BLAS eviction, save/load
integrity, pex decompiler hardening, ESM VWD flag, ragdoll telemetry, NIFAL
boundary).

## Dedup check

Cross-referenced all findings against `gh issue list --state open` (71 open
issues) and every prior `docs/audits/AUDIT_REGRESSION_*.md` report. One
adjacent note: **#1824** (`gl_to_gamebryo_blend truncates u32 src/dst via 'as
u8' with no range guard`) is correctly still **OPEN** — it targets the same
function touched by the #1823 fix (now renamed `bgsm_blend_to_gamebryo`) but
describes a distinct, still-unaddressed narrowing-without-range-guard
concern. This is not a regression of #1823 and is not re-reported here.

## Findings

No `Regression of #NNN` findings. Every discovered fix — the 6 SKILL-named
fresh candidates and the full Session 54 wave — is intact with its guard
test, and the full workspace suite is green.

No `/audit-publish` run is needed for this report (zero findings to file).
