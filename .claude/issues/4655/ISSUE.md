# PAR-D1-2026-09-21-02: HKX MAX_TRANSFORM_SAMPLES is absolute: a 17 KB clip decodes to 610 MiB and is retained as about 2 GB of keys

Labels: medium,bug,import-pipeline,game:skyrim

## Description
`crates/hkx/src/animation.rs:52` defines `MAX_TRANSFORM_SAMPLES: usize = 16_000_000` as an absolute cap on `transform_count * num_frames` (checked at `:338-358`). #3011 bounded that product to stop allocator aborts, but the bound is absolute rather than tied to the data actually present in the file. Static tracks cost 0 bytes per frame in the on-disk encoding, so decoded output size is decoupled from file size.

`convert_hkx_clip` (`:444-452`, called from `byroredux/src/asset_provider/animation.rs:392-474`) then expands each sample into `TranslationKey` (56 B), `RotationKey` (48 B) and `ScaleKey` (32 B), about 136 B/sample on the default glam layout, and the result is registered in `AnimationClipRegistry` for the session — a persistent, not transient, cost.

Verified unchanged at HEAD `ee6d3fb39`: the constant is still `16_000_000`, and there is still no `num_frames <= num_blocks * (max_frames_per_block - 1) + 1` style relative check.

## Evidence
Probe `hkx-samples`:

```
spline clip file 16,929 B: Ok, 4096 tracks x 3906 frames = 15,998,976 samples (40 B each = 610 MB) in 1.37 s;
    VmHWM 3,376 kB -> 651,988 kB
```

99 tracks x 161,616 frames binds fully to the vanilla 99-bone skeleton. That gives about 2.18e9 bytes of converted keys per clip, on top of the transient 610 MiB decode.

## Impact
One mod-replaced clip pins about 2 GB for the session. The cart catalogue alone installs up to 16 clips, so a few such files exhaust RAM on a 16 GB machine. Main thread, Skyrim only.

## Related
#3011 (original sample-count bomb fix — this is the same bound's absoluteness, not a regression of it), PAR-D1-2026-09-21-01 (companion HKX Size Discipline finding)

## Suggested Fix
- Require `num_frames <= num_blocks * (max_frames_per_block - 1) + 1`.
- Cap `max_frames_per_block` and `num_frames` at a measured vanilla ceiling (plus headroom), so output stays proportional to the per-block data the file must actually carry.
- Optionally budget converted keys per clip.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D1-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix