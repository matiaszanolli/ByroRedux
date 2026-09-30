# #4875: REN-D8-2026-09-24-05: volumetrics doc rot after 0572bfd5a — ray budget 14 vs 22, 'uncommitted' status doc, shader-pipeline gaps

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D8-2026-09-24-05**._

**Severity**: LOW · **Status**: NEW · every claim below was checked against the code by the auditor and re-checked against the tip of `main` at publish time.

**Dimension**: Volumetrics

the inject shader header and `volumetrics.rs` (`VOLUMETRIC_OUTPUT_CONSUMED`) say "up to 14 ray queries per froxel … ~12.9M/frame" assuming 4 lights, but `MAX_FROXEL_LIGHTS` is 8: the real worst case is 22 (18 at the default tier 2) plus up to 9 combustion boundary queries in transport regions (~20.3M at 160×90×64, ~45.6M at 1080p); `interior-godrays-status.md` still says "uncommitted" though it ships in `0572bfd5a`; `shader-pipeline.md` omits the sun-portal lane, the aperture UBO, the 64 + 128 per-cluster index budget and `render_origin.w` as the sky-access bit.

## Completeness Checks
- [ ] **SIBLING**: The same stale claim is checked in the neighbouring docs and code comments (doc moves with the pass)

