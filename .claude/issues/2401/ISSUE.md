# #2401 — CHAIN2-D2-02: Caustic parked-camera EMA counts global frames while each FIF slot only accumulates every other frame

- **Severity**: LOW
- **Domain**: renderer
- **Audit**: `docs/audits/AUDIT_CONCURRENCY_2026-08-07.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2401


- **Severity**: LOW
- **Dimension**: 2 — Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/caustic.rs:743-756`
- **Status**: NEW

**Description**

The caustic accumulator is per-FIF (`self.slots[frame].image`), not ping-ponged, and never cross-seeded between slots. The decay factor is derived from `self.parked_frames`, a single counter bumped once per `dispatch` call (once per global frame). At `MAX_FRAMES_IN_FLIGHT == 2` a given slot is only visited every other frame, so on its k-th visit it is decayed with `n = 2k-1` and admits new energy with weight `1/(2k)` after only `k` real samples — the estimator converges at roughly `1/√k` instead of the intended `1/k`.

**Evidence** (re-confirmed at publish time against commit `79bfc76e`): `parked_frames` incremented once per `dispatch`; `decay_factor = (n/(n+1)).min(CAUSTIC_DECAY_MAX)` computed from that shared counter but applied to `self.slots[frame].image`, a per-FIF, never-cross-seeded image.

```rust
if camera_static {
    self.parked_frames = self.parked_frames.saturating_add(1);
} else {
    self.parked_frames = 0;
}
let slot_img = self.slots[frame].image;
let decay_factor = if camera_static {
    let n = self.parked_frames as f32;
    (n / (n + 1.0)).min(CAUSTIC_DECAY_MAX)
} else {
    0.0
};
```

**Impact**

No bias (still converges), but visible as residual half-rate shimmer on caustic pools for the first ~2 seconds of a parked camera — the exact artifact the EMA was added to remove. No synchronization hazard; the per-FIF fence fully covers the decay→splat read-modify-write chain.

**Trigger Conditions**: Camera parked with a refractive caustic source on screen; observe the first ~2 seconds of convergence.

**Verification Path**: Not a validation-layer issue — reproduce visually (`--cornell` or a glass-heavy interior) with a parked camera, or log `parked_frames`/`decay_factor` per frame against per-slot visit count.

**Related**: `#321` (Option A caustic splat), CHAIN2-D2-01 (same file family).

**Suggested Fix**: Make the parked counter per-FIF (`parked_frames: [u32; MAX_FRAMES_IN_FLIGHT]`) so `n` counts that slot's own visits, or seed one slot from the other. The former is a two-line change.

## Completeness Checks
- [ ] **SIBLING**: Check other per-FIF EMA/accumulator patterns (SVGF temporal, TAA) for the same shared-counter-vs-per-slot-visit mismatch
- [ ] **TESTS**: A test or logged-metric check confirming per-slot `n` now matches that slot's actual visit count after the fix

---
Filed from `docs/audits/AUDIT_CONCURRENCY_2026-08-07.md` via `/audit-publish`.
