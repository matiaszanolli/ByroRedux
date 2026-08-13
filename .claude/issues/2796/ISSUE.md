# REN-D16-01: bloom source attachment has no sky, so the pyramid is seeded with the exterior clear colour and real sky/sun radiance never blooms

- **Severity**: MEDIUM
- **Dimension**: 16 — Bloom
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs` (`record_bloom_pass` → `bloom.dispatch(&self.device, cmd, frame, hdr_view)` with `hdr_view = composite.hdr_image_views[frame]`); `crates/renderer/shaders/composite.frag` (`is_sky` branch, and the later unconditional `combined += bloom * BLOOM_INTENSITY`); `byroredux/src/main.rs` (`clear_color`); `crates/core/src/types.rs` (`Color::CORNFLOWER_BLUE`)
- **Status note**: follow-up to CLOSED #2233, not a regression of it (#2233's fix, the unconditional add, is intact; the source is the gap).
- **Description**: Sky is synthesised inside `composite.frag` and never exists in the HDR G-buffer attachment bloom reads. (1) The sun disc and bright sky can never bloom — #2233 made the composite add unconditional precisely so sky pixels would receive bloom, but the pyramid holds no sky radiance. (2) A debug clear colour is injected: exterior `clear_color = CORNFLOWER_BLUE = (0.392, 0.584, 0.929)`, so every sky pixel of the bloom source is that constant.
- **Evidence**: `record_bloom_pass` doc states input is "the raw pre-TAA HDR attachment"; `rebind_hdr_views` never rewrites `hdr_image_views`. `composite.frag`'s `is_sky` branch never reads `hdrTex`. Per `bloom_upsample.comp`'s documented DC gain (`up[0] = 5V`), composite adds ≈ (0.29, 0.44, 0.70) linear HDR to sky pixels, upstream of ACES — a genuine exposure lift.
- **Impact**: Exterior-only. Sky washed toward blue and lifted ~0.3–0.7 linear; glow bleeds around horizon geometry; the #2233 effect is absent exactly where it matters most. Structural half established from code; magnitude figure is analytic, wants a capture.
- **Related**: #2233 (CLOSED), #2466 (OPEN), REN-D13-02 (third consumer of same root condition), #1166, #1107, REN-D16-06.
- **Suggested Fix**: Either clear the exterior HDR attachment to black (accept sky doesn't bloom), or move bloom downstream of composite into its own HDR pass so it sees sky/GI/caustics.

## Completeness Checks
- [ ] TESTS: A regression test pins this specific fix
- [ ] Needs a capture to confirm the magnitude before sizing which fix direction to take

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2796
