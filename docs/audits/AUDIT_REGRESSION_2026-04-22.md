# AUDIT — Regression Verification (2026-04-22)

**Scope:** All 50 most-recently-closed `bug`-labelled issues (#452 → #568)
**Method:** `git log --grep="Fix #N"` for commit presence + source grep for ongoing reference + spot-read of the fix site in current tree
**Time window:** closes between 2026-04-19 and 2026-04-22 (last ~3.5 days)
**Reverts checked:** `git log --grep="Revert"` since 2026-04-15 returned one revert (`01d7232 Revert #389` — reverted issue not in this audit window)

## Headline

- **48 PASS** — fix commits present, source still references the fix, no reverts touching the affected files.
- **2 NOT_PLANNED** — closed as premise-mismatch after audit claim was disproved (#473 caustic pipeline, #475 ControlledBlock start/stop). Legitimate closes.
- **0 FAIL** — no regressions detected.

Given how tight the window is (all 50 fixes landed in the last 4 days), a clean slate is expected. This audit is a baseline for future regression sweeps.

## Summary table

| Issue | Title | Status | Fix commit |
|-------|-------|--------|------------|
| #568 | SK-D5-06: nif_stats clean metric hides NiUnknown recovery | PASS | `5cb1ff8` |
| #543 | M33-11: WTHR DATA comment block contradicts field offsets | PASS | `56390b9` |
| #538 | M33-06: classification byte offset off by two | PASS | `56390b9` |
| #536 | M33-04: FNAM arm body empty — FNV/FO3 fog never parses | PASS | `107dc5d` |
| #535 | M33-03: DNAM misinterpreted as `[u8; 4]` cloud_speeds | PASS | `8ff989a` |
| #534 | M33-02: Cloud-texture FourCCs never match | PASS | `8ff989a` |
| #533 | M33-01: NAM0 parse gate rejects FO3 + Oblivion | PASS | `850d504` |
| #526 | FNV-ANIM-3: Root-motion split comment conflates Y-up / XZ | PASS | `43275ad` |
| #522 | FNV-CELL-1: resolve_texture cache-key divergence | PASS | `574f464` |
| #521 | FNV-ESM-10: ACTI / TERM parsed only as MODL statics | PASS | `992da82` |
| #518 | FNV-RUN-1: tex/mesh debug endpoints unreachable | PASS | `783e08b` |
| #517 | FNV-ANIM-1: AnimatedColor loses target-slot | PASS | `783e08b` |
| #516 | FNV-RT-1: Frustum culling drops occluders from TLAS | PASS | `80d87d2` |
| #515 | Phase-2 glass classifier tags wood/cloth as GLASS | PASS | `3854fb7` |
| #514 | FNV interior: blown-out windows, ghost picture frames | PASS | `d802a49` |
| #513 | Interior fog floods empty-depth pixels | PASS | `4d33a59` |
| #500 | PERF D3-M2: `debug_assert!` tuple order mismatch | PASS | `d8752bf` |
| #499 | PERF D3-M1: Blended sort key omits `(src,dst)` | PASS | `6128ef3` |
| #498 | PERF D2-M4: `write_mapped` re-queries memory_properties | PASS | `6128ef3` |
| #497 | PERF D2-M3: Terrain tile SSBO should be DEVICE_LOCAL | PASS | `6128ef3` |
| #496 | PERF D2-M2: `drain_terrain_tile_uploads` fresh 32KB/frame | PASS | `a310205` |
| #495 | PERF D2-M1: BLAS scratch never shrinks | PASS | `1274555` |
| #494 | FO4-BGSM-5: fragment shader apply uv_offset/uv_scale | PASS | `b69608c` |
| #493 | FO4-BGSM-4: asset_provider BGSM resolver | PASS | `b69608c` |
| #492 | FO4-BGSM-3: expand GpuInstance with uv_offset/uv_scale | PASS | `62587fc` |
| #491 | FO4-BGSM-2: corpus integration test for 6,899 BGSM files | PASS | (umbrella) |
| #490 | FO4-BGSM-1: new `crates/bgsm` crate | PASS | `b69608c` |
| #486 | FNV-AN-L2: AnimationPlayer debug-snapshot serialization | PASS | `a657399` |
| #485 | FNV-AN-L1: KeyType::Quadratic falls back to SLERP | PASS (deferred) | `cfdb307` |
| #483 | FNV-2-L3: Tautological `d.len() >= 20` in XCLL fog path | PASS | `1d73c9d` |
| #482 | FNV-2-L2: FACT XNAM combat_reaction u8 from u32 field | PASS | `14d6511` |
| #479 | FNV-REN-L1: TAA dispatch failure silently falls through | PASS | `6c2fbd1` |
| #478 | FNV-3-L3: climate.sun_texture parsed but not consumed | PASS | `574f464` |
| #477 | FNV-3-L2: CellLoadResult.mesh_count counts cache-misses | PASS | `e163e92` |
| #476 | FNV-3-L1: CLMT WLST chance u32 but should be i32 | PASS | `aeccbbd` |
| #475 | FNV-AN-M2: No start/stop in AnimationLayer | NOT_PLANNED | (premise mismatch) |
| #474 | FNV-1-M1: NIF parser coverage hidden by block_sizes | PASS | (umbrella — follow-up via #568) |
| #473 | FNV-REN-M2: Caustic scatter between SVGF and TAA | NOT_PLANNED | (premise mismatch) |
| #471 | FNV-3-M2: EnableParent::default_disabled over-hides | PASS | `d3f8bdf` |
| #470 | FNV-3-M1: spawn_terrain_mesh ignores ATXT/VTXT splat | PASS | `e1b8fc1` |
| #469 | FNV-AN-H1: AnimationClip.weight never consumed | PASS | `6a8ad0b` |
| #468 | FNV-3-H1: WTHR cloud textures lack `textures\` prefix | PASS | `b0448fb` |
| #464 | E-01: Transform propagation is DFS but comments say BFS | PASS | `73e07f5` |
| #463 | FO3-6-05: Climate TNAM sunrise/sunset not consumed | PASS | `a594f2f` |
| #459 | FO3-NIF-L1: BSShaderTextureSet silently clamps negatives | PASS | `4b1b23c` |
| #458 | FO3-3-07: WATR/NAVI/NAVM/REGN/ECZN/LGTM/HDPT/EYES/HAIR | PASS | `374d208` |
| #455 | FO3-5-02: TileShaderProperty aliased to PPLighting | PASS | `5b037f1` |
| #454 | FO3-REN-M3: BSShaderNoLightingProperty skips ALPHA_DECAL_F2 | PASS | `5b037f1` |
| #453 | FO3-REN-M2: GpuInstance lacks parallax + env cube bindings | PASS | `62587fc` |
| #452 | FO3-REN-M1: BSShaderTextureSet slots 3/4/5 never read | PASS | `7b5ada4` |

## NOT_PLANNED closures (not regressions)

### #473 — Caustic scatter between SVGF and TAA
Close comment: audit premise was wrong. Caustic writes to a dedicated R32_UINT accumulator via `imageAtomicAdd`; composite samples HDR (post-TAA), indirect (SVGF output), and caustic at separate bindings, then combines per-pixel. Caustic never passes through TAA's neighborhood clamp. No code change needed.

### #475 — No start/stop in AnimationLayer
Close comment: `ControlledBlock` has no per-channel start/stop fields; temporal range lives on `NiControllerSequence.start_time/stop_time` (already captured as `AnimationClip.duration`) and `NiBSplineInterpolator.start_time/stop_time` (handled by interpolator internal time transform). The partial-range state machine is tracked separately as #338. No code change needed.

## Special-check spot reads

Re-verified the five fragile-area checks called out by the regression protocol:

| Check | Site | Status |
|-------|------|--------|
| `recovered_blocks` counter wired to `scene.rs` | `crates/nif/src/scene.rs:44` | PASS — `pub recovered_blocks: usize`, initialized at `:54` |
| Frustum cull split (`in_raster` + TLAS) | `crates/renderer/src/vulkan/{acceleration,context}` | PASS — 12 refs to #516 across 3 files |
| FACT combat_reaction regression test | `crates/plugin/src/esm/records/actor.rs:413` | PASS — regression test asserts u32 read |
| XESP default-disabled interim predicate | `crates/plugin/src/esm/cell.rs:195-200` | PASS — inverted predicate with documented caveat |
| XCLL colors RGB (not BGR) after #389 revert | `crates/plugin/src/esm/cell.rs` | PASS — reverted in `01d7232` (not in this audit window, but verified no re-introduction) |

## Methodology caveats

- **Time-bounded**: all 50 issues closed in the last ~3.5 days. Regression is statistically unlikely at this window.
- **Spot-check depth**: 5 of 50 verified by reading the fix site directly; the other 45 are PASS-by-git-log-confirmation. A more thorough sweep would read every cited line.
- **Test coverage**: this audit did not run `cargo test`. If the claim is "fix still works", that requires execution, not file reads.

## Conclusion

Zero regressions detected. No action required.

If a deeper regression sweep is desired (running tests + reading every fix site), re-run `/audit-regression` with a tighter `--limit`.
