# #1812: D6-05: First-sight entities pay a redundant BLAS UPDATE immediately after their fresh BUILD in the same command buffer

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (D6-05)
**Location**: `crates/renderer/src/vulkan/context/draw.rs:1848-1870,1878`

## Description
A first-sight entity is always dirty, so the refit-gate condition is false and
the loop proceeds to `refit_skinned_blas` — a full UPDATE against the identical
vertex data the BUILD consumed moments earlier in the same command buffer. The
block comment asserts the fall-through is harmless, but `refit_skinned_blas` has
no "freshly built this frame" short-circuit and records a real UPDATE, also
inflating `refits_attempted`/`refits_succeeded` on spawn frames. Note: this same
BUILD-then-UPDATE-in-one-cmd-buffer adjacency is also the trigger condition for
the separate Vulkan-spec barrier bug tracked in #1790 (missing `AS_READ` on the
scratch-serialize barrier) — implementing this finding's fix (skip the redundant
refit entirely on freshly-built entities) would incidentally close that hazard
window too, but the two are distinct findings: this one is a wasted-work/perf
issue, #1790 is a formal synchronization-correctness issue.

## Impact
One redundant AS UPDATE + barrier per skinned entity per spawn frame only;
steady-state unaffected. Minor telemetry skew on spawn frames.

## Suggested Fix
Track entities built this frame and `continue` past the refit for them; fix the
stale comment either way.

---

# #1813: PERF-D5-NEW-03: SVGF a-trous recomputes the 5x5 spatial-variance estimate in all 5 iterations

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D5-NEW-03)
**Location**: `crates/renderer/shaders/svgf_atrous.comp:134-150` (unconditional 5x5
luminance loop); dispatch loop `crates/renderer/src/vulkan/svgf.rs:88`
(`ATROUS_ITERATIONS = 5`), `:1258-1288`

## Description
Each of the 5 a-trous iterations re-derives a 5x5 local luminance variance
(`spatialVar`, `svgf_atrous.comp:134-150`, no iteration-index gate) and re-runs
the 3x3 temporal-variance prefilter (`:108-123`) before the 25-tap edge-stopped
filter. The spatial estimate is a legitimate iteration-0 concern (catches
converged-but-noisy pixels), but iterations 1-4 run it against already-filtered
color whose local variance shrinks monotonically, mostly duplicating work with
diminishing contribution.

## Impact
Constant-factor bandwidth/ALU on 5 full-screen dispatches (~460M extra, heavily
L2-cached, texel fetches/frame at 1440p); the pass remains strictly O(pixels).
Confidence: HIGH on the cost; the safety of computing spatial-variance once and
propagating it needs a visual A/B against the dark-floor moiré regression scene
before shipping.

## Suggested Fix
Compute the spatial-variance estimate in iteration 0 only, propagate through the
unused moments-image channel, falling back to temporal-variance-only weight in
later iterations.
