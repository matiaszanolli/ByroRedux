# REN-D16-01: memory-budget.md's volumetrics VRAM row understates the froxel grid by ~24x, breaking the documented 4 GB ceiling at 4K

**Issue**: #3117 — https://github.com/matiaszanolli/ByroRedux/issues/3117
**Labels**: `high,renderer,memory,documentation`
**Filed**: 2026-08-20 · comprehensive audit suite
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-20.md (merged with PERF-D3-01)`

---

**Severity**: HIGH
**Dimension**: Volumetrics / GPU Memory Pressure
**Source**: `docs/audits/AUDIT_RENDERER_2026-08-20.md` (REN-D16-01) — found **independently** by `docs/audits/AUDIT_PERFORMANCE_2026-08-20.md` (PERF-D3-01) in the same suite sweep. Filed once here; the two reports' evidence is merged below because each found something the other did not.

## Location

- `docs/engine/memory-budget.md` — the "Volumetrics (M55)" section (`:228-256`) **and** the `VRAM Rough Budget` ledger row (`:467`) + estimated-total row (`:475`)
- `crates/renderer/src/vulkan/volumetrics.rs` — `VolumetricsPipeline::new` (the six `Vec<FroxelSlot>` fields at `:740-796`, all six pushed once per `MAX_FRAMES_IN_FLIGHT` at `:905-990`), `FROXEL_FORMAT` / `COMBUSTION_FIELD_FORMAT` / `EMISSION_HISTORY_FORMAT` (`:532/538/543`), the boot log line at `:1601-1612`
- `crates/renderer/src/vulkan/upscaling.rs` — `VolumetricsConfig::default` (`:113-118`)

## Description

The volumetric froxel grid grew along **two independent axes** inside this delta and neither growth reached the VRAM ledger.

**(a) Froxel count 4×.** `VolumetricsConfig::default`'s `froxel_xy_divisor` went **8 → 4** in `0ff7b537` (2026-08-17), quadrupling the froxel count. `validate`'s lower bound was simultaneously relaxed from 4 to 2.

**(b) Volumes per slot 2 → 6.** The per-FIF volume set went from two (`lighting_volumes`, `integrated_volumes`) to **six** — the same two plus `emission_history_volumes` (`R32_SFLOAT`, 4 B/froxel), `combustion_state_volumes`, `combustion_dynamics_volumes` and `combustion_optical_volumes` (all `R16G16B16A16_SFLOAT`, 8 B/froxel). Per-froxel cost per FIF is therefore **44 B**, not the documented 8 B × 2 volumes = 16 B. The four uncounted volumes are the combustion-transport field; the last three landed in this delta (`0ff7b537` → `4a35819e`).

**The document contradicts itself.** The prose in the Volumetrics section was updated for (a) but still reads *"Two volumes per frame (lighting + integrated) × 2 FIF"* and derives its whole table from `… × 8 B × 2 volumes × 2 FIF`. The `VRAM Rough Budget` ledger row two hundred lines below was never updated for **either** change — it still reads:

```
| Volumetrics froxel grid (2 volumes, 2 FIF) | ~29.5 MB (1080p) | ~118 MB (4K) |
```

which is the figure for the **pre-Session-62** fixed 160×90×128, two-volume grid. So the ledger row is a further **~9×** below the document's own detail section, and both numbers are below the truth.

## Evidence

Six `Self::create_volume(...)` calls inside the `for i in 0..MAX_FRAMES_IN_FLIGHT` loop in `VolumetricsPipeline::new`, pushing to six distinct `Vec<FroxelSlot>` fields; the code's own comment above that loop reads "Six volumes per frame". `MAX_FRAMES_IN_FLIGHT = 2` (`sync.rs`).

**The code already knows the right number and prints it at boot.** `volumetrics.rs:1601` logs `"… {} MiB / slot (5×RGBA16F + R32F) …"`, computing `w*h*d*44 / (1024*1024)`. The doc is the only place that still says 16 B.

Confirmed at HEAD:
```
crates/renderer/src/vulkan/upscaling.rs:115:            froxel_xy_divisor: 4,
crates/renderer/src/vulkan/upscaling.rs:417:        assert_eq!(config.froxel_xy_divisor, 4);
docs/engine/memory-budget.md:467:| Volumetrics froxel grid (2 volumes, 2 FIF) | ~29.5 MB (1080p) | ~118 MB (4K) |
docs/engine/memory-budget.md:474:| **Estimated total** | **~1.59 GB** | **< 4 GB target** |
```

`froxel_extent` = `render.{width,height}.div_ceil(4) × froxel_z_slices (64)`. Arithmetic on the doc's own decimal-MB basis:

| Render extent | Froxels | Ledger row `:467` | Section table (2 volumes) | **Actual (6 volumes, 44 B, 2 FIF)** |
|---|---|---:|---:|---:|
| 1920×1080 | 480×270×64 = 8 294 400 | ~29.5 MB | ~265.4 MB | **~730 MB** |
| 2560×1440 | 640×360×64 = 14 745 600 | — | ~471.9 MB | **~1.30 GB** |
| 3840×2160 | 960×540×64 = 33 177 600 | ~118 MB | ~1061.7 MB | **~2.92 GB** |

That is a **24.7×** understatement against the ledger row the audit skills designate as authoritative for VRAM ceilings, and 2.75× against the doc's own detail section.

The pipeline is created unconditionally (`context/mod.rs:2447`; failure only on a Vulkan error), so this is resident in **every** session, not an opt-in feature.

The commit that made the divisor change describes it as *"Adjusted froxel grid configuration to improve memory usage and performance"*; at a fixed `froxel_z_slices`, halving the XY divisor does the opposite by 4×.

## Impact

- The ledger's `**Estimated total** | **~1.59 GB**` becomes **~2.29 GB** at 1080p native once volumetrics is counted correctly.
- At 4K the volumetrics grid **alone** (2.92 GB) consumes ~73% of the stated `< 4 GB target` before ReSTIR's 531 MB, SVGF's 332 MB, textures or BLAS — the peak column no longer describes a reachable configuration. **The documented < 4 GB target is broken by volumetrics alone on the 12 GB dev GPU.**
- The grid keys on **render** extent, so FSR Quality at 1080p output softens it to ~324 MB — still 11× the ledger row. That mitigation is what the doc should state; it currently states the undercount instead.
- On the documented 6 GB RT-minimum card (`feedback_vram_baseline.md`) this is the difference between comfortable and tripping the allocator's own 80%-of-heap warning.
- Because `/audit-performance` Dimension 3 explicitly forbids re-deriving ceilings (*"Do NOT re-derive memory ceilings — `docs/engine/memory-budget.md` is the authoritative source"*), every future audit and every sizing decision that cites the doc inherits the error.

Nothing here is a leak or a correctness bug — the cost is real, intended and freed correctly (`destroy` drains all six volume vectors; the resize destroy/recreate path is clean). What is broken is that the project's authoritative VRAM analysis no longer describes the engine.

## Suggested Fix

1. Rewrite `memory-budget.md:228-256` to enumerate all six volumes with their formats and derive from **44 B/froxel/slot**; correct the section table.
2. Update the ledger row at `:467` and the estimated-total row at `:475`.
3. State the FSR render-extent mitigation explicitly.
4. **Separately, confirm the 8 → 4 divisor default was an intentional quality decision rather than a sign flip** — it is a 4× VRAM *and* 4× inject-dispatch-workload change shipped under a commit message describing a memory improvement, and it is not covered by any bench-of-record refresh. This half is a decision, not an edit.
5. Consider whether the three combustion fields need to be allocated at all when no `FogVolume` with a transport profile has ever existed in the session — a lazily-created combustion sub-group would return ~400 MB at 1080p to scenes that never see fire. (See the runtime half of the same observation, filed separately as the `volumetrics_inject.comp` combustion-gate finding.)

## Related

- #2801, #2679 (same class of ledger drift, both CLOSED) · #2242 (`REN-D16-04`, CLOSED — same file's fog-volume path)
- `docs/engine/memory-budget.md`, `feedback_vram_baseline.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other screen-sized / FIF-doubled ledger rows — ReSTIR, SVGF, bloom, caustic accumulators)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (e.g. assert the doc's stated bytes-per-froxel against `volumetrics.rs`'s own boot-log arithmetic, so the ledger cannot silently desync again)
