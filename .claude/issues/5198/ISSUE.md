# #5198 — REN-D11-2026-10-03-01: the #5158 change of the adaptation constant from 0.2 s to 0.5 s has no effect; `ExposureTuning::default()` still holds 0.2 s and overwrites the renderer every frame

**Labels**: medium,renderer,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: MEDIUM
- **Dimension**: FSR/Presentation
- **Location**:
  - `ExposureTuning::default` in `byroredux/src/components.rs` sets `adaptation_seconds: 0.2`.
  - The per-frame push in `App::render_one_frame`, `byroredux/src/app_frame.rs`: `ctx.exposure_adaptation_seconds = exposure.adaptation_seconds;`.
  - `VulkanContext::new` in `crates/renderer/src/vulkan/context/init.rs` sets `exposure_adaptation_seconds: 0.5`.
- **Status**: NEW. It was introduced by 7d99ba7f0. Related to the OPEN #5158, which this commit was partly meant to fix.
- **Description**:
  - 7d99ba7f0 raised the renderer's eye-adaptation constant from 0.2 s to 0.5 s. Its stated aim was to stop per-frame meter jitter from the MC residual pumping the exposure (#5158).
  - The engine does not run with the renderer's own field, though:
    - `boot::build_world` always inserts `ExposureTuning::default()`.
    - `App::new` reseeds only `auto` and `agx` from `RendererConfig`.
    - `render_one_frame` copies `ExposureTuning.adaptation_seconds` into `ctx.exposure_adaptation_seconds` unconditionally on every frame, including frame 0.
  - The live constant is therefore still 0.2 s, and the 0.5 s in `init.rs` is dead.
  - The `ExposureTuning` docstring says "the resource defaults match the renderer's own", which is no longer true.
  - Two comments now describe a default that does not run:
    - the #4590 comment in `build_and_upload_instances.rs`: "(1.0 s at the 0.5 s default, N = 2)";
    - the loading-cover comment in `app_frame.rs`: "auto adaptation resumes (0.5 s τ)".
- **Evidence**:
  - `components.rs`: `impl Default for ExposureTuning { … adaptation_seconds: 0.2, … }`. Blame: d54382415, which predates 7d99ba7f0.
  - `app_frame.rs`: inside `if let Some(exposure) = self.world.try_resource::<ExposureTuning>()`, the line `ctx.exposure_adaptation_seconds = exposure.adaptation_seconds;`.
  - `main.rs` `App::new`: only `tuning.auto = …; tuning.agx = …;`.
  - The `exposure` console status prints `speed = 0.20 s` at boot.
  - No test ties the two defaults together. `git grep adaptation_seconds -- byroredux/src` finds only the struct, the console command and the push.
- **Impact**:
  - The anti-pumping half of the #5158 tuning never shipped.
  - Any A/B or live calibration that credits its result to the "0.5 s τ" measured 0.2 s.
  - Workaround: run `exposure speed 0.5` each session.
  - `fixed_exposure: 0.85` is a second hand-typed copy of `DEFAULT_EXPOSURE`. It matches today, but it is the same drift class.
- **Related**: #5158 (OPEN), #4590 (per-FIF alpha), d54382415.
- **Suggested Fix**:
  - Add a `DEFAULT_ADAPTATION_SECONDS` const in `crates/renderer/src/vulkan/exposure.rs`. Use it in both `init.rs` and `ExposureTuning::default`, and make `fixed_exposure` read `DEFAULT_EXPOSURE`.
  - Add a bin test that asserts `ExposureTuning::default()` matches the renderer constants.
  - Correct the two comments.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix
