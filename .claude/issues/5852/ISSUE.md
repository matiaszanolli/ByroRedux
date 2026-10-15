# TD8-2026-10-15-05: The grass-LOD rewrite orphaned the frame-serial plumbing: `GC_FRAME_SERIAL` has no reader, but host packing, a push-constant bit-field and a test still maintain it

Labels: low,renderer,shaders,terrain-exterior,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD8-2026-10-15-05`).

- **Severity**: LOW
- **Dimension**: 8 - Dead Code & Backwards-Compat Cruft (shader + host)
- **Location**: `crates/renderer/shaders/groundcover_blade.vert:106-111` (`#define GC_FRAME_SERIAL`), `crates/renderer/src/vulkan/groundcover/frame.rs:122-137` (packs `serial << 2 | tier`), `crates/renderer/src/vulkan/groundcover.rs:359-360` (`GROUNDCOVER_FRAME_SERIAL_BITS/_MASK`), `:770-830` (the test that pins the define's text and the f32 round-trip); also `crates/renderer/shaders/include/groundcover_density.glsl:89` (`byroGcWorley`, a wrapper over the used `byroGcWorleyCell`)
- **Status**: NEW
- **Age**: `af83238810` (2026-10-14, "Grass LOD: per-chunk 3/2/1-segment tiers") — its diff deletes `vFrameSerial = GC_FRAME_SERIAL;` and the `blueNoiseRankAt` use; `byroGcWorley` from `39c9e39f0` (10-14)
- **Effort**: small
- **Description**: a whole-tree identifier count over the shader directory finds exactly two symbols with a single occurrence: `GC_FRAME_SERIAL` (defined, never read) and `byroGcWorley`. The first matters: the host still masks `floor(time*60)` to 22 bits, multiplies by the tier stride, and a test asserts the shader `#define` text byte-for-byte (`groundcover.rs:785-792`), while the frame.rs comment still says the serial "makes the blue-noise transition repeat under a replayed simulation clock" — a consumer that no longer exists. #4498's f32-exactness fix stays correct for the tier bits, but the serial half is dead weight in the push-constant word.
- **Evidence**: `git show af8323881 -- crates/renderer/shaders | grep FRAME_SERIAL` (the `-    vFrameSerial = GC_FRAME_SERIAL;` hunk); `git grep -nw GC_FRAME_SERIAL` -> define + the Rust test string only.
- **Impact**: low runtime cost; the risk is a future reader trusting the comment and a test that keeps a dead shader contract alive. Removing the define does not change SPIR-V (unused macro), but anything editing `groundcover_blade.vert` must re-run `glslangValidator` per *feedback_spv_rebuild_staleness* and the shader-struct lockstep rules — do not touch the tier packing.
- **Related**: #4498 (closed, serial exactness), M-grass LOD work.
- **Suggested Fix**: drop the define, `GROUNDCOVER_FRAME_SERIAL_*`, the serial term in `frame.rs` and its test assertions (keep tier encoding + its f32-exact round-trip); delete `byroGcWorley` (or call it from `byroGcWorleyCell`'s only consumer if intended).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
