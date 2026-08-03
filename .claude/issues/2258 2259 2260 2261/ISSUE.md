# Issues 2258, 2259, 2260, 2261

## #2258 — TD1-080: record_post_passes is a 556-LOC single function covering 8+ distinct GPU passes inline, zero test coverage

**Location**: `crates/renderer/src/vulkan/context/post_passes.rs:137-693` (`record_post_passes`)

Inline-records water-caustic barrier, SVGF temporal+spatial, SSAO, bloom down/up, volumetrics inject/integrate, composite, TAA, FSR upscale, and presentation passes back-to-back. Same natural next-level split as the file-level #1857 split that created this file.

**Fix**: extract each self-contained pass block into its own `fn record_<pass>_pass(&mut self, cmd, frame, ...)` helper, called in sequence. Purely call-order-preserving — don't reorder barriers or passes.

## #2259 — TD1-081: build_tlas is an ~835-LOC single function — long-standing debt, resurfaces the 05-13 TD9-012 finding at a higher LOC count

**Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs:46-880` (`build_tlas`)

One `unsafe fn` covering instance-buffer sizing/rebuild, scratch-buffer growth/shrink decisions, per-draw-command instance transform + shadow-mask + custom-index assembly, and BUILD-vs-UPDATE dispatch. Slow steady growth (834→887 LOC over ~3 months), first flagged 2026-05-13 pre-file-split, never re-tracked.

**Fix**: mirror `blas_static.rs`/`blas_skinned.rs`'s style (named helpers pulled to `predicates.rs`, already imported here) — extract the instance-buffer rebuild/resize block and the per-draw-command instance-assembly loop into private helpers, keeping `build_tlas` as the sequencing function.

## #2260 — TD2-101: sample_scalar / sample_color in cinematic.rs duplicate the same keyed-linear-interpolation control flow for two value types

**Location**: `crates/scripting/src/cinematic.rs:420-464` (`sample_scalar`, `sample_color`)

Added by `4598bc74` (IMAD sampling path). Structurally identical: early-return-on-empty, before-first-key holds first value, `keys.windows(2)` scan for bracketing pair, width/alpha clamp-and-lerp, after-last-key holds last value. Only the interpolated field type (`f32` vs `[f32;4]`) differs.

**Fix**: extract a single generic helper (e.g. `sample_keyed<T, V>` with `time_of`/`value_of`/`lerp` closures, or a `Lerp` trait for `f32`/`[f32;4]`) serving both call sites.

## #2261 — TD4-001: _audit-common.md's "22-crate roster" is stale — crates/hkx is missing, live count is 23

**Location**: `.claude/commands/_audit-common.md:120-126` (crate count paragraph); `.claude/commands/audit-tech-debt/SKILL.md:21` ("the 22-crate roster")

`crates/hkx` (`byroredux-hkx`, Session 62 M47.2 MQ101 cinematic slice) is absent from both the enumerated list and the count. ROADMAP.md is already correct (26 workspace members = 23 crates + binary + 2 tools).

**Fix**: add `hkx` to the enumerated list + its own Project Layout row (mirroring `fsr3-sys`'s treatment); bump "22" to "23" in both files.

## Domain classification
- #2258: renderer (byroredux-renderer) — Vulkan, pure refactor, LOW severity
- #2259: renderer (byroredux-renderer) — Vulkan, pure refactor, LOW severity
- #2260: scripting (byroredux-scripting) — pure Rust logic dedup
- #2261: documentation only — `.claude/commands/` markdown files, no code/crate target
