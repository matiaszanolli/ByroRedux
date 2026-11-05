# RF0-2026-11-05: Refactor backlog from the 2026-11-05 review (tracker)

Labels: low,tech-debt,enhancement

This tracks the 40 issues split out of the 2026-11-05 refactor review. Each covers one part or component, and none is meant to change behaviour. None of it is urgent: the problems are duplicated logic, oversized state structs and fragile guard tests, not bad module boundaries.

**PRD**: not required. `docs/prd/README.md` §1 lists refactors and file splits as exempt. #6702 adds a scheduler API, which is the closest call, but it is internal and adds no host-facing contract. The owner can still ask for a PRD (trigger T5).

## Where to start
1. #6681: a small, low-risk fix, and the least-protected shader duplicate left.
2. #6684: makes schedule-order guards trustworthy before anything moves.
3. #6690: the highest-value item, because these copies can change behaviour when they drift.

## Issues by area

| Area | Effort | Risk | Issues |
|---|---|---|---|
| 1. Mesh spawn paths | Medium | Medium | 3 |
| 2. Shader structs (core shipped in `b8233b4b2`) | Small | Low | 3 |
| 3. `App` and `VulkanContext` state | Medium | Low to medium | 8 |
| 4. Source-text guards | Incremental | Low | 6 |
| 5. Subsystem wiring (children of #5802) | Large | Medium | 7 |
| 6. Vulkan recording functions (children of #6143) | Medium | High | 3 |
| 7. Binary crate layering | Small to medium | Low | 7 |
| 8. Parallel scheduling | Unknown until profiled | Medium | 1 |
| Side notes: comments and line budgets | Small | Low | 2 |

### 1. Mesh spawn paths
- [ ] #6690 Merge the shared material-to-component sequence
- [ ] #6691 Dismemberment-cap filter is on one path only
- [ ] #6692 Retire the spawn-parity text guards

### 2. Shader structs
- [ ] #6681 `MorphDeltaRef` / `MorphWeightRef` copies
- [ ] #6682 `CameraUBO` copies
- [ ] #6683 Field parser's one-declaration-per-line blind spot

### 3. State structs
- [ ] #6693 `App`: bench fields
- [ ] #6694 `App`: ground-cover fields
- [ ] #6695 `App`: UI and HUD fields
- [ ] #6696 `App`: capture and debug-output fields
- [ ] #6697 `VulkanContext`: skinning and morph fields
- [ ] #6698 `VulkanContext`: exposure settings and pass-failure state
- [ ] #6699 `VulkanContext`: ray-probe and image-health diagnostics
- [ ] #6700 `VulkanContext`: ground-cover and terrain-tile state

### 4. Source-text guards
- [ ] #6684 Schedule-order guards move to `access_report()`
- [ ] #6685 `acquired_in` parses instead of searching text
- [ ] #6686 Triage: binary crate
- [ ] #6687 Triage: renderer
- [ ] #6688 Triage: scripting
- [ ] #6689 Triage: NIF, plugin and physics

### 5. Subsystem wiring
- [ ] #6701 `Access::with` for delegated declarations
- [ ] #6702 Named ordering constraints in the scheduler
- [ ] #6703 Late-stage registration
- [ ] #6704 Update and PostUpdate registration
- [ ] #6705 `build_save_registry` rows
- [ ] #6706 `scripting::register` resource setup
- [ ] #6707 Shared helper for 22 `flush_*` methods

### 6. Vulkan recording functions
- [ ] #6708 `record_skinned_blas_refit`
- [ ] #6709 `build_blas_batched`
- [ ] #6710 `record_geometry_pass`

### 7. Binary crate layering
- [ ] #6711 `components` upward references
- [ ] #6712 `material_translate` flag packers and constants
- [ ] #6713 Coordinate helpers
- [ ] #6714 `render` upward references
- [ ] #6715 `systems` to `cell_loader` shared types
- [ ] #6716 `populate_scene_runtime` out of `asset_provider`
- [ ] #6717 `cell_loader` to `systems` light translation and helpers

### 8. Parallel scheduling
- [ ] #6718 Profile first, then promote only what shows up

### Side notes
- [ ] #6719 Issue history in production comments
- [ ] #6720 Function-size budgets count comments

## Dependencies

| Do first | Then |
|---|---|
| #6690 | #6692, and #6691 if the filter joins the shared step |
| #6684, #6701, #6702 | #6703, then #6704 |
| #6685 | #6707 (or extend `acquired_in` in the same commit) |
| #6371 | #6708, #6709, #6710 |
| #6697 | #6708 |
| #6714, #6715 | #6717 |

## Not filed

| Item | Why |
|---|---|
| The main shader-struct move | Shipped in `b8233b4b2` |
| Compute-pass teardown | Already tracked by #6371 |
| `apply_buoyancy_with_scratch`, `spawn_synth_child`, `render_one_frame` | Remain under #6143 |
| `GpuBoundaryInstance` | A deliberate prefix mirror, pinned field by field and by stride |
| `GcCameraUBO` | Its own block, not a copy of `CameraUBO` |
| Splitting big files on size alone, or splitting the binary crate | The review advised against both for now |
| Translation boundaries | They have no per-game branches, so that design is holding |

## Changes from the review
Re-measuring at `02f46f95d` changed these points:

| Point | Review | Measured |
|---|---|---|
| Source scans | 1,034 | 1,048 `include_str!` reads of Rust files: 895 standalone scans and 153 `SOURCES` concat rows |
| Scripting scans | Not broken out (124 reads in total) | 39 standalone |
| Spawn-path overlap | 11 component types, 69 identical lines | 13 component types, 65 identical lines |
| Wrong-way dependencies | 2 named | Also `render`, `asset_provider` and a `systems` / `cell_loader` cycle, so #6713 to #6717 were added |
| PRD gate | Items 3-5 might trigger it | Refactors are exempt under §1 |

Counts come from line and regex scripts. The profiling in #6718 has not been done, so nothing here shows that the serial schedule costs frame time.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6680 --json state`)._
