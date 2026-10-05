# #5316: PAR-D1-2026-10-05-02: After #5006, a 279 KB HKX clip still decodes to 15.7 M samples (597 MiB), because the absolute cap sits 128× above the vanilla maximum

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5316
- **Labels**: medium,import-pipeline,bug,animation,memory,safety,game:skyrim
- **Source**: `docs/audits/AUDIT_PARSERS_2026-10-05.md` (PAR-D1-2026-10-05-02)

_From `docs/audits/AUDIT_PARSERS_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: MEDIUM
- **Dimension**: Size Discipline
- **Location**: `crates/hkx/src/animation.rs:52` (`MAX_TRANSFORM_SAMPLES` = 16,000,000), `crates/hkx/src/animation.rs:68` (`MAX_FRAMES_PER_BLOCK` = 256), `crates/hkx/src/animation.rs:355-384` (dimension gate), `crates/hkx/src/animation.rs:472-481` (sample expansion). Consumer: `byroredux/src/asset_provider/animation.rs:403` (`convert_hkx_clip`).
- **Status**: NEW. This is a residual after #5006, whose specific bypass is closed. It descends from #3011 and #4655.
- **Trigger Input**: an `hkaSplineCompressedAnimation` with `num_blocks = 4096`, `max_frames_per_block = 256`, `num_frames = 4096 × 255 + 1`, all masks 0, and blocks chained at exactly `mask_size` bytes each.
- **Description**:
  - #5006's cap makes each block cost its full `transform_count × 4` mask table. That ties output to file size at about 64 samples (about 2.5 KB decoded) per file byte, whatever the track count.
  - The only absolute bound remains `MAX_TRANSFORM_SAMPLES` = 16 M. The census added with the fix now measures vanilla's largest clip at **124,821** samples (`paired_dlc1seranaholdsvyrthur.hkx`, 201 tracks × 621 frames). The cap is therefore 128× vanilla.
  - The fix comment claims "decoded output stays proportional to file bytes". That is true only with a 2,500× constant.
- **Evidence** (probe `hkxprobe`, a hand-built 64-bit Havok 2010 packfile through `decode_spline_animation`):
  ```
  T=15 blocks=4096 mfpb=256: file   279,041 B -> Ok, 15 tracks x 1,044,481 frames = 15,667,215 samples (597 MiB) in 0.40 s
  T=15 blocks=4096 mfpb=257: file   279,041 B -> Err(InvalidData("unsupported spline clip dimensions"))
  T=99 blocks=4096 mfpb=40:  file 1,655,297 B -> Ok, 99 tracks x 159,745 frames = 15,814,755 samples (603 MiB) in 0.46 s
  VmHWM: 735,460 kB
  ```
- **Impact**:
  - The impact is the same as PAR-D1-2026-09-29-01 and the original #4655: about 600 MiB peak during the decode.
  - The 99-track form binds to the vanilla skeleton, so `convert_hkx_clip` keeps it as keys for the session (about 2 GB per clip, as measured on 09-21).
  - It runs on the main thread, on Skyrim only. The trigger is one mod-overridden `.hkx` of an ordinary size: a 279 KB to 1.7 MB file is unremarkable for a paired animation.
- **Related**: #5006, #4655, #3011, PAR-D1-2026-09-29-01
- **Suggested Fix**:
  - Set `MAX_TRANSFORM_SAMPLES` from the census, for example 1 M (8× the vanilla maximum).
  - Have `skyrim_se_spline_dimensions_census_stays_under_the_gate_ceilings` assert `max_samples` against it, so a vanilla re-export that grows trips the real-data gate.
  - Add the probe's 279 KB case as a unit test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
