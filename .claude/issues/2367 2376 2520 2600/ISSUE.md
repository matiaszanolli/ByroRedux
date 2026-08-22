# Issue #2367: PERF-REGRESSION-3a02b02d..28155b79 — FO4 scenes ~33-34% slower, needs bisection

**Severity**: MEDIUM · **Domain**: performance (needs bisection, not yet root-caused)

Bench-of-record refresh found real regressions on two FO4 interiors (MedTek
~33%, Dugout Inn ~34% slower TAA frame time) at flat entity count, and a real
~2x improvement on Prospector (FNV) plus a mild ~20% slowdown on the
synthetic Cornell control, across 119 commits (`3a02b02d..28155b79`,
Sessions 60-62: procedural volumetric fog, clustered local fog volumes,
material-aware path-traced GI, materials-pipeline refactor, streaming
resumability). Whiterun's entity count also grew +51% unexplained over the
same range.

**Explicitly not a code-fix issue yet** — its own completeness checklist says
"TESTS: N/A until root-caused — this is a measurement/bisection issue, not a
code-fix issue yet." Requires `scripts/fsr-bench-matrix.sh` bisection runs
against a live Vulkan device + real FO4/FNV game archives.

---

# Issue #2376: EX-06/07 — Exterior boundary benchmark and deadline-bounded streaming

**Severity**: HIGH · **Domain**: ecs + renderer (terrain-exterior/EXAL)

Plan-shaped issue (not a scoped bug): build a deterministic camera/player
path crossing ≥2 cell boundaries, instrument emit/parse/apply/unload/LOD/
frame p50/p95/max timings per cell, convert every NIF finalization / static
placement / terrain-water-precombine phase / texture-mesh upload / BLAS
build / LOD provider yield from attempt-count budgeting to a shared
wall-clock deadline, detect hitches above a threshold and name the largest
atomic unit, and run on FNV WastelandNV + one newer worldspace.

This is a milestone-sized feature plan spanning streaming, terrain,
rendering upload, and LOD subsystems — not a single-site or few-file fix.

---

# Issue #2520: REN-D23-2026-08-07-04 — TAA-mode FrameUpscaler pays a byte-identical full-res blit

**Severity**: LOW · **Domain**: renderer (Vulkan, FSR upscaler)

`FrameUpscaler::record_native_blit` (`crates/renderer/src/vulkan/frame_upscaler.rs`):
in `UpscalerMode::Taa`, `FrameExtentSet::for_output` sets `render == output`,
so the blit's src/dst offsets are identical and the `LINEAR` filter
degenerates to an exact copy — every TAA-mode frame reads+writes a full-res
R16G16B16A16_SFLOAT image (~16MB @1080p, ~66MB @4K) plus two pipeline
barriers, purely to move data into a target `presentation.frag` could sample
directly. The module doc calls the split deliberate (decouples scene
composition/presentation, gives FSR one explicit frame-graph slot) but
doesn't document the cost.

**Suggested fix**: either let `PresentationPipeline` bind composite's scene
view directly when `render == output` and skip the blit, or (simpler, since
the issue says don't re-bench FSR off the back of this) document the cost in
the module doc. Completeness checklist: TESTS N/A unless the blit-skip is
implemented, in which case bench the bandwidth saving.

---

# Issue #2600: FO4-D4-04 — bs_sub_index parsed and cloned per mesh with zero readers

**Severity**: LOW · **Domain**: nif (NIF parser)

`crates/nif/src/import/mesh/bs_tri_shape.rs:208-211` parses and deep-clones
`bs_sub_index` into `ImportedMesh` on every mesh import; no call site
anywhere reads it back out. Deliberate per existing docs (reserved for a
future consumer) but wastes allocations proportional to mesh count.

**Suggested fix**: either gate the parse+clone behind the eventual consumer
landing, or skip the clone and retain only a cheap presence flag until
needed. Completeness: TESTS N/A — perf/allocation cleanup, no behavior
change if done correctly.
