# Performance issue fix status — 2026-09-26

Scope: the 29 open `performance` issues in `matiaszanolli/ByroRedux`, refreshed
from GitHub on this date. Earlier work from this task is already in commit
`88c23887b`; subsequent fixes are in the working tree based on `f5e54295e`.
This report distinguishes implemented changes from unresolved design or
measurement requirements. No issues have been closed or commented on by this
task, and no changes have been pushed.

“Implemented” below means the reported mechanism has been changed and its
regression checks pass. It does **not** assert a measured frame-time improvement.
The estimates in the original issues are not measurements of these fixes.

## Issue-by-issue review

All issue numbers link to `https://github.com/matiaszanolli/ByroRedux/issues/<number>`.

| Issue | Disposition | Implementation and verification evidence |
| --- | --- | --- |
| [4866](https://github.com/matiaszanolli/ByroRedux/issues/4866) | Implemented | Ground-cover models have a COMPUTE start and BOTTOM end enclosing PLACE/LAYOUT/EMIT and stats copy. Timer decoding, active flags, telemetry consumers and benchmark keys are covered. `model_timer_encloses_all_phases_and_stats_copy` fails when the start call is removed, then passes when restored. Exposure metering has its own sibling bracket. |
| [4824](https://github.com/matiaszanolli/ByroRedux/issues/4824) | Implemented | `npc_spawn/ai_package.rs` filters Dead and combat actors before cloning candidate stacks. Separate query scopes avoid new lock edges. The minute remains unset so combat can end within the same minute and ambient behavior resumes immediately. Source-order and combat-resumption tests cover both requirements; saved shapes are unchanged. |
| [4810](https://github.com/matiaszanolli/ByroRedux/issues/4810) | Implemented | `pipeline_compile`, `parameter_upload`, `fog_cluster`, and `post_present` spans reach frame stats, debug output, metrics and bench output. Each variant compile logs its duration. Fog preparation is subtracted from command-record time. CPU consumer guards cover the new fields. |
| [4809](https://github.com/matiaszanolli/ByroRedux/issues/4809) | Implemented | `CachedNifImport::beam_volumes` retains positive and negative results in a `OnceLock`; placements share the immutable volume arrays. Debug-only path classification is gated. `beam_classification_reuses_positive_and_negative_model_results` proves allocation reuse across repeated placements. |
| [4808](https://github.com/matiaszanolli/ByroRedux/issues/4808) | Code implemented; A/B measurement outstanding | Main-render timing starts at COMPUTE instead of TOP; the FSR TSV includes sky-cube, TLAS, cluster-cull and RT-tier columns. Existing results must be re-recorded on both sides using this harness before drawing performance conclusions. |
| [4807](https://github.com/matiaszanolli/ByroRedux/issues/4807) | Partial; measurement required | Zero-extinction LightShaft entries retain portal references without occupying density slots. Telemetry reports maximum density/portal counts. The saturation test proves empty apertures cannot displace smoke; even tiny positive authored density remains. Positive-density cutoffs and cap sizing remain unmeasured. |
| [4806](https://github.com/matiaszanolli/ByroRedux/issues/4806) | Suggested code fix implemented | Composite apertures are culled before upload; CPU packing computes camera-local origins and conservative screen bounds. Pixels reject bounds before rotation. The 48-byte aperture stride and UBO limit remain intact. Ray-hit equivalence tests cover camera rebasing, rotations, Y flips and eye-plane crossings. The existing bounded background-depth opening probe remains; this change does not claim its removal or a Nellis visual A/B. |
| [4805](https://github.com/matiaszanolli/ByroRedux/issues/4805) | Implemented | Composite uploads stop at the live aperture prefix. Sky-cube parameters are built directly, and the outdoor sky palette is stored by value. Layout/upload-size tests cover the ABI and live prefix. |
| [4804](https://github.com/matiaszanolli/ByroRedux/issues/4804) | Implemented | `StaticBlasWorkingSet` uses dense handle-indexed generation stamps instead of a hash set. Both recovery and actual TLAS collection refresh membership; tests cover batches, changing visibility and idle eviction. |
| [4803](https://github.com/matiaszanolli/ByroRedux/issues/4803) | Implemented | Geometry lookup returns a fingerprint reused for fresh-batch deduplication and registration, including NPC and Cornell paths. Full geometry equality still resolves collisions. The index-sanitization regression verifies lookup/registration fingerprint agreement. |
| [4797](https://github.com/matiaszanolli/ByroRedux/issues/4797) | Implemented | `WaterSurfaceMesh` builds a bounded XZ grid over shared triangles. Large triangles use a separate candidate list. Tests compare against a full scan at grid edges, holes, overlapping layers, source-order ties, degenerate and nonfinite input. Existing camera/player/physics callers use the same component method. |
| [4796](https://github.com/matiaszanolli/ByroRedux/issues/4796) | Implemented | Both NIF bulk callers use a concrete `Cursor<&[u8]>` helper. It validates byte count and input range before one copy into aligned Vec capacity, then sets length and cursor position. No zero-fill or scratch allocation remains. The local unsafe trait explicitly forbids padding. Tests cover unaligned input, count overflow, EOF and cursor preservation; a dhat test pins one 256 KiB allocation for a 256 KiB array. |
| [4795](https://github.com/matiaszanolli/ByroRedux/issues/4795) | Measurement required | Corrected the stale fit-projection documentation. No speculative eviction policy change: the issue explicitly asks to measure moving-camera churn before adding hysteresis. Details below. |
| [4794](https://github.com/matiaszanolli/ByroRedux/issues/4794) | Implemented | Shared bounded tone-mip cache; common immutable NIF import cache for inspection and spawn hooks; private copy-on-write actor deformation; in-place EGM conversion; neighbor vertex buckets. Tests prove tone-sample equivalence, cache reuse/bounds, preserved neighbor order, and zero extra import-cache parses for repeated hand hooks. Material resolution remains part of the cached import. |
| [4793](https://github.com/matiaszanolli/ByroRedux/issues/4793) | Partial; cache design and measurement required | The whole sun term is gated by live radiance/scattering. Tier 0 now skips unmarked rim detection while retaining authored apertures, glass and opaque visibility. Shader/host header tests pin tier publication. Coarse verdict caching and noon/midnight comparison remain outstanding. |
| [4789](https://github.com/matiaszanolli/ByroRedux/issues/4789) | Implemented | Separate inject/integrate timers, COMPUTE-stage starts, and fence-associated volumetrics state reach debug/metrics/bench/TSV consumers. State includes tier, cap, froxel extent, transport activity, volume count and cluster maxima. Timer/consumer tests pass. This enables attribution; it does not replace controlled A/B measurements. |
| [4788](https://github.com/matiaszanolli/ByroRedux/issues/4788) | Implemented | Cluster writes use the union of current and previous per-slot `[lo, hi)` ranges with byte offsets; indices use their live prefix. Tests cover untouched prefixes, disappearing volumes, stale-count clearing and repacking. |
| [4787](https://github.com/matiaszanolli/ByroRedux/issues/4787) | Implemented | `sampleLocalMedium` rejects transported profiles before density/noise evaluation. Combustion shader contract tests cover the ordering. |
| [4786](https://github.com/matiaszanolli/ByroRedux/issues/4786) | Implemented | Per-slot empty-field proofs become valid only after a submitted clear. Known-empty slots skip transport history reads/stores and source/moment work. Tests cover abandoned recordings, both slots, resumed emitters and paused-but-active fields. Dirty moment state remains latched until drained. |
| [4785](https://github.com/matiaszanolli/ByroRedux/issues/4785) | Implemented | Sun visibility is gated on positive sun radiance and scattering. The local-light block now also skips visibility rays and soot marches when scattering is zero. Emission/authored inscatter remain outside that gate. Both sun and local shader guards pass; SPIR-V rebuilt. |
| [4784](https://github.com/matiaszanolli/ByroRedux/issues/4784) | Partial; occupancy design required | Curl/wind forcing is gated on positive activity. The RK2 and neighbor-gather work still runs on quiet froxels while transport is active elsewhere. A correct occupancy/history design is required for the remaining optimization; see below. |
| [4783](https://github.com/matiaszanolli/ByroRedux/issues/4783) | Implemented | Renderer-side grid filtering precedes dispatch/transport decisions and clustering. It retains intersecting bounds and remote apertures whose sun sweep reaches the grid. Tests show distant flames cannot arm transport, nearby flames can, and culling does not erase existing linger. |
| [4209](https://github.com/matiaszanolli/ByroRedux/issues/4209) | Implemented | Screenshot claim/wait returns publish all three `about_to_wait` timings through the shared helper. A regression checks both return paths and actual resource values. |
| [4208](https://github.com/matiaszanolli/ByroRedux/issues/4208) | Implemented | CPU timing documentation now describes nested intervals and actual boundaries. The debug UI no longer sums nested CPU spans into a misleading total; metrics/debug descriptions match the producers. |
| [3813](https://github.com/matiaszanolli/ByroRedux/issues/3813) | Design required | Per-plugin parallel parsing remains unimplemented. Its issue explicitly requires a dedicated ordering/shared-state design, real multi-master equality oracle and new benchmark. Concrete findings below. |
| [3659](https://github.com/matiaszanolli/ByroRedux/issues/3659) | Code implemented; streaming measurement outstanding | BA2 reads packed GNRL/DX10 payloads under the file guard and decompresses after dropping it, matching BSA. Poison recovery is preserved. The new source guard pins both paths and their I/O helpers; raw/LZ4 and zlib extraction tests pass. Streaming comments now describe both backends. No FO4 moving-streaming hitch reduction is claimed. |
| [3477](https://github.com/matiaszanolli/ByroRedux/issues/3477) | Implemented | Physics caches a proven-complete registration set using structural generations, not equal row counts. Tests cover equal-count membership changes, handle removal and late body/transform arrival. The cache resource guard is dropped before acquiring component queries. |
| [3475](https://github.com/matiaszanolli/ByroRedux/issues/3475) | Implemented | Interaction selection snapshots each storage in separate scopes and retains row/target capacity. Tests cover bound/transform fallback order, lock cycles, keys, disabled references, corpses and line-of-sight. Queries are released before physics/world lookups. |
| [3429](https://github.com/matiaszanolli/ByroRedux/issues/3429) | Implemented | Same-size RGBA updates retain their image/view/descriptors, coalesce CPU pixels and copy through a retained staging arena per frame slot in the normal frame command buffer. Barriers protect earlier reads and later consumers. Successful submission alone acknowledges pixels; released handles and registry teardown reclaim retained storage. Both HUD families passed Vulkan synchronization validation. Initial creation and resizing still use the synchronous allocation path. |

## Remaining design and measurement work

### #4784 — conservative occupancy across transport history

The existing cluster list describes current emitters, not all advected medium.
An emitter-only mask would delete lingering smoke. A previous-GPU mask needs
the coordinate frame and completion state of the history it describes:

1. Keep the history's world-grid origin and frame/slot generation with its mask;
   remap or conservatively invalidate on recenter, render-origin shift, history
   reset and skipped/abandoned submission.
2. OR current source bounds into that mask before deciding a cell is empty.
3. Dilate for the actual elapsed simulation step **and** trilinear history /
   neighbor-gather support. The issue's 1.87 m motion estimate alone does not
   prove the support bound of every history sample after reprojection.
4. Publish GPU writes and any clear in an explicit buffer dependency; never
   reuse an in-flight mask or treat recorded-but-unsubmitted data as valid.
5. Compare full-grid and masked transport with moving camera, thin plumes,
   disappearing/reappearing emitters, pause, expiry and grid-edge advection.
   Then compare inject timings at the same pinned tier and render extent.

This is a new temporal GPU data path. The existing activity gate is useful,
but does not constitute completion of the occupancy request.

### #4793 — cached architectural rim verdicts

Rim verdicts depend on position, sun direction and available architectural
geometry. A single verdict reused across a coarse block can cross a window or
wall boundary. A cache needs a defined spatial error tolerance or a conservative
fallback for mixed blocks, plus invalidation for sun motion, grid/rebase changes,
streamed architecture and BLAS/TLAS availability. Preserve explicit-window
behavior and tier-0 shedding. Verify missing ceilings and partial rims at noon
and midnight with pinned `--rt-test-ray-quality-tier`, fixed camera and equal
render extent; report inject time and image differences. None of those A/B
measurements has been performed here.

### #3813 — parallel plugin parse, ordered merge

Current `records/parse.rs::parse_esm_with_load_order` constructs a private reader,
maps and an `EsmIndex`. Its localized-string state is a scoped thread-local guard;
the `Rc` record trace is unwrapped before returning. These are promising worker
boundaries, but are not a completed audit of all dispatched parsers.

`EsmIndex::merge_from` has different ordering rules: last nonempty plugin chooses
`game` (#3403), first non-NONE profile chooses `character_rules` (#3384), later
record maps override earlier maps, and tombstones/partial cell overrides have
their own merge behavior. Parallel completion order must never determine merge
order. Compute remaps first; return indexed per-plugin results; retain the exact
sequential fold, error behavior and post-merge classification. Audit dispatch
helpers for shared state, bound simultaneous input/index memory, and compare
every field against sequential parsing of an actual multi-master load order,
including overrides/deletions/localized plugins. Re-measure cold/warm load time
and peak memory. No parallel parser or equality oracle is claimed in this patch.

### #4795 — moving-camera BLAS recovery

The stale documentation is fixed. Compare old/new admission policies with a
low test budget, identical camera travel crossing the RT distance ring, and
identical content. Record restore time, builds, evictions, resident bytes and
RT completeness. Static-camera convergence does not answer this issue. Only if
the moving-camera comparison demonstrates churn should a re-admission cooldown
be selected; all frees must continue through deferred BLAS destruction. No
hysteresis value or claimed win is inferred from the unit tests.

### #4807, #4808 and #3659 — quantitative follow-up

- **#4807:** use the new cluster maxima and inject timing on dense fire and beam
  scenes at pinned tiers. Sweep caps/candidate thresholds with visual comparison.
  A positive density threshold changes authored light scattering; zero-only
  rejection is the sole lossless cutoff established here.
- **#4808:** re-run both comparison revisions with the same updated harness,
  scene state, extent, tier, warmup and shader cache. Old main-pass measurements
  include a different timing boundary and cannot establish an A/B result.
- **#3659:** compare BA2-backed FO4 exterior streaming with the same path and
  workload, using apply-slice percentiles and CPU frame buckets. Source inspection
  proves that inflate is outside the mutex; it does not measure contention or
  hitch reduction on that workload.

## Verification

Rust verification uses the installed 1.96.0 toolchain's cargo directly, with its
bin directory first in PATH. No workspace-wide formatting was applied.

- Affected library suites: core 774 passed; NIF 1,362 passed / 1 ignored; BSA
  98 passed / 11 ignored; physics 188 passed; renderer 1,237 passed / 1 ignored;
  debug UI 16 passed.
- Full application unit suite: 2,444 passed / 46 ignored, including existing
  save-schema and lock-order checks; no saved component/resource format was
  changed by these optimizations.
- NIF dhat allocation bound: exactly one 262,144-byte allocation for 65,536 u32s.
- Ground-cover timer mutation: removing the production START call produces the
  expected regression failure; restoring it passes.
- `composite.frag.spv` and `volumetrics_inject.comp.spv` rebuilt from GLSL; shader
  layout/reflection guards pass.
- Oblivion MenuXml smoke: health fill changed from 158 to 53 pixels at a 35% pin.
- Skyrim Scaleform smoke: 2,088 changed compass pixels versus 409 in the control
  region when hiding chrome. Both runs enabled Vulkan synchronization validation
  and reported no VUID or synchronization hazards.

Smoke checks establish functioning GPU uploads and validation for those scenes;
they are not performance benchmarks or comprehensive visual equivalence tests.
The first two HUD runs preceded the final local-light and rim-tier gates.
The Oblivion smoke was repeated after rebuilding the final shaders and engine:
the same 158-to-53-pixel gate passed, with synchronization validation enabled
and zero VUID/SYNC-HAZARD lines (`/tmp/tmp.JcgP2QvJTy/engine.log`).

Detailed command output is retained locally in `/tmp/performance-*.log`, including
`performance-final-libraries.log`, `performance-final-renderer-bsa.log`,
`performance-final-application.log`, `performance-nif-allocation.log`, and
`performance-model-timer-{mutation,restored}.log`.
