**Source**: [`docs/audits/AUDIT_CONCURRENCY_2026-05-17.md`](docs/audits/AUDIT_CONCURRENCY_2026-05-17.md)
**Dimension**: Vulkan Sync (image-source mis-binding, not a barrier hazard)
**Severity**: MEDIUM

## Observation

`crates/renderer/src/vulkan/context/draw.rs:2312-2329`:

```rust
// Bloom pyramid (M58). Reads the post-TAA resolved HDR
// (composite.hdr_image_views[frame] — TAA writes its output
// here, so bloom is post-TAA; #1107 / REN-D19-002) and writes
// a multi-scale blurred bright-content texture. ...
// Bloom uses TAA-jittered input but the blur pyramid
// suppresses sub-pixel jitter — visually equivalent to
// bloom on TAA output but with simpler wiring.
if let Some(ref mut bloom) = self.bloom {
    if let Some(ref composite) = self.composite {
        let hdr_view = composite.hdr_image_views[frame];
        if let Err(e) = bloom.dispatch(&self.device, cmd, frame, hdr_view) {
            ...
        }
    }
}
```

The comment is internally contradictory and the first half is factually wrong:

- **Claim**: "TAA writes its output [to `composite.hdr_image_views[frame]`]" — **false**. TAA writes to `self.history[f].view` (see `taa.rs:546-548`, descriptor binding 5 `out_taa`).
- **Claim**: "bloom is post-TAA" — **false**. Bloom reads `composite.hdr_image_views[frame]`, which is the raw G-buffer HDR attachment from the main render pass. Composite has its binding 0 rebound to `taa.output_view(i)` at `context/mod.rs:1713-1716` so that composite samples TAA, but the **bloom binding is never rewired**.

The later lines of the comment ("Bloom uses TAA-jittered input ... visually equivalent") rationalize the actual behaviour, contradicting the lead claim.

## Why it's a bug

Two-part issue:

1. **Wiring may or may not be intentional**: Bloom is computed from jittered, AA-free HDR; post-bloom output is then mixed into composite (which samples the de-jittered TAA result). On high-contrast geometry, the bloom pyramid contains the TAA jitter signal, creating a low-amplitude shimmer in bloom haloes. Below "obvious by inspection" — **needs RenderDoc validation** to confirm whether the visual artifact is observable.
2. **Comment is misleading**: future maintainers reasoning about "TAA → bloom" ordering will be confused. If the wiring is intentional, the comment needs rewriting; if not, the wiring is the bug.

## Fix

Choose one:

**Option A** (rewire to TAA output): Add a `bloom.rebind_hdr_views(...)` mirror of `composite.rebind_hdr_views` and call it from `context/mod.rs:1715` with `taa_views` + layout `GENERAL` (matching `taa.rs:606-636` `initialize_layouts`). Bloom now reads AA'd input; comment becomes truthful.

**Option B** (keep wiring, fix comment): Rewrite the comment to "bloom intentionally reads the pre-TAA raw HDR attachment so the pyramid sees only spatial signal, not temporal jitter — TAA's resolved output is consumed by composite separately. The bloom result is added to the composite's TAA-sampled HDR, so the final image has spatial bloom over temporally-stable base color."

## Completeness Checks

- [ ] **UNSAFE**: N/A (descriptor binding only)
- [ ] **SIBLING**: SSAO has the same "did the rebind cover this consumer?" surface — verify SSAO reads the intended HDR slot (likely depth-only, but worth confirming)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: RenderDoc capture of a high-contrast HDR scene with TAA on/off comparing bloom output

## Related

- #1107 / REN-D19-002 — cited in the comment; appears to be the issue that added the (misleading) comment
