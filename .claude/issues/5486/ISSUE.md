# #5486: NIF-D6-2026-10-09-01: B-spline key sampling is capped per channel but not in aggregate. One shared interpolator with a huge `stop_time` costs ~118 MiB per ~29-byte controlled block, enough to OOM-abort the stream worker from a ~4 KB NIF.

**Labels**: animation, bug, high, nif, nif-parser, safety

**Source**: `docs/audits/AUDIT_NIF_2026-10-09.md` — finding `NIF-D6-2026-10-09-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: HIGH. The allocation is sized from on-disk fields (`stop_time`, controlled-block count) with no aggregate cap, and the result is an uncatchable abort. Precedents are #4317 (HIGH: 330 KB `.pex` → 8 GB abort) and #408 (HIGH).
- **Dimension**: Allocation Hygiene (import-side amplification on the `pre_parse_cell` path)
- **Game Affected**: all titles whose NIFs carry `NiControllerSequence` + `NiBSplineComp*Interpolator` (FO3/FNV onwards). The KF path (`import_kf` on `.kf` files) shares it.
- **Location**:
  - `crates/nif/src/anim/bspline.rs:283-284` and `:316-318` (transform: `n_samples = (duration * 30).ceil().clamp(2, 1_000_000)` from the **interpolator's own** `stop_time - start_time`; three `Vec::with_capacity(n_samples)`);
  - `:226-230` (float);
  - `crates/nif/src/anim/channel.rs:311-315` (color);
  - `crates/nif/src/anim/sequence.rs:47-66` (one channel per controlled block, with no reuse of a shared interpolator);
  - `crates/nif/src/anim/entry.rs:26` (`import_kf` imports **every** sequence) and `:822-845` (`import_embedded_animations_with_sequences` keeps the first);
  - worker call site: `byroredux/src/streaming/pre_parse.rs:302-303`.
- **Status**: NEW. #408 (closed) added the per-channel 1 M clamp, aimed at "`usize::MAX` slots". Nothing bounds the sum across channels, sequences or a shared interpolator. No open or closed issue matches.
- **Description**: several things combine:
  - Each transform channel is resampled at 30 Hz over the interpolator's `[start_time, stop_time]`. The span is not clamped to the owning sequence's span: a sequence with `stop_time = 1.0` still yields 1 M samples.
  - Even with no control data, where every handle is invalid and the pose fallback applies, every sample pushes a translation, a rotation and a scale key. Those keys are 56 B, 36 B and 32 B, so ~124 B per sample.
  - Controlled blocks can all reference one interpolator. Each produces its own channel, and distinct node names keep them all alive in the clip's map.
  - `import_kf` builds every sequence before the first one is chosen.
- **Evidence**: the `bspline_amp` scratch bin builds a synthetic scene through the public `byroredux_nif::anim::import_kf`. It has one `NiBSplineCompTransformInterpolator` (`stop_time = 1e9`), a 4-control-point basis, an empty `NiBSplineData`, and one sequence (`stop_time = 1.0`) with N controlled blocks. Results (`/tmp/audit/nif/bspline_amp.log`):

  ```
  controlled_blocks=1 ... total_keys=3000000  VmHWM delta=118MiB
  controlled_blocks=2 ... total_keys=6000000  VmHWM delta=236MiB
  controlled_blocks=4 ... total_keys=12000000 VmHWM delta=473MiB
  ```

  A 20.2.0.7 controlled block is 29 bytes on disk: 4 + 4 + 1 + 5×4 string indices. About 128 of them (~4 KB) therefore come to ~15 GB.
- **Impact**:
  - On the stream pool this is an allocation failure, and an allocation failure aborts the process. `parse_one_nif`'s `catch_unwind` cannot catch it, and up to 32 tasks run concurrently.
  - The same NIF on the synchronous path (`references/import.rs`) aborts on the main thread.
  - Vanilla content is unaffected; #408's own comment notes real animations top out at a few thousand samples. The fix is defense in depth against mod or corrupt content.
- **Related**: #408, #4317, #155. Sibling: the per-channel caps in `channel.rs` and `bspline.rs` (float).
- **Suggested Fix**:
  1. Clamp each interpolator's sampled span to the owning sequence's `[start_time, stop_time]`.
  2. Lower the per-channel ceiling to a realistic bound. For example, 30 Hz × 1 h = 108 k.
  3. Add a per-`import_kf` aggregate key budget that declines further B-spline channels once it is exceeded.
  4. Optionally, cache sampled keys per interpolator index so a shared interpolator is sampled once.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
