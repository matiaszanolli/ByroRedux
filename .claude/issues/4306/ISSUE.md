# #4306: REN-2026-09-14-D8-02: `composite.frag`'s binding comments still describe a 32-step froxel ray-march, a pre-ACES bloom add and a tone-map that this shader no longer performs

- **Labels**: low,renderer,shaders,doc-rot,documentation
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4306
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/shaders/composite.frag` (comments on `volumetricFroxel` binding 6, `bloomTex` binding 7, and the geometry arm of `main`)
- **Status**: NEW
- **Description**: Three comment claims contradict the live shader and, in one case, the same file:
  1. The binding-6 comment says the froxel volume is "Sampled per-fragment with a 32-step ray-march for the in-scatter + transmittance modulation applied to `combined` before ACES."
     - The live consumer is `sampleVolumetricColumn`: a depth-weighted 2×2 bilateral column tap with one linear Z blend. No march.
     - No ACES runs in this shader; it lives in `presentation.frag`.
  2. The binding-7 comment says bloom mip 0 is "added to `combined` before ACES per Frostbite §8." The same file's M58 block, near the end of `main`, says `bloomTex` "is therefore unused by this shader now" (#2796). `bloom_apply.comp` performs the add.
  3. The geometry-arm comment reads "combine direct + (indirect × albedo) and tone map". No tone map follows.
- **Evidence**: `grep -n "ACES\|32-step\|tone map" crates/renderer/shaders/composite.frag`, compared against the body of `sampleVolumetricColumn` and the "`bloomTex` (binding 7) is therefore unused" comment in `main`.
- **Impact**: Documentation only. The header is the first thing a reader of the composite contract sees, and it re-teaches the composite-does-ACES misconception that open #4202 is tracking in `CLAUDE.md`, plus the pre-#2796 bloom placement.
- **Related**: #4202 (open, same misattribution in `CLAUDE.md`, different file), #3608 (closed, `renderer.md` bloom attribution), #2796.
- **Suggested Fix**: Reword the binding 6/7 comments: a depth-weighted column tap, output linear HDR with tone mapping in `presentation.frag`, and `bloomTex` declared-but-unused because `bloom_apply.comp` adds bloom. Drop "and tone map" from the geometry-arm comment.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
