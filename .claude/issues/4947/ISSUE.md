# #4947: REN-D11-2026-09-27-01: a persisted `render.upscaler` silently replaces the CLI default; the boot log line, and the determinism harness that scrapes it, report the pre-override value

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4947
- **Labels**: medium,renderer,tech-debt,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D11-2026-09-27-01**._

- **Severity**: MEDIUM. There is no GPU fault, but it defeats the one signal every harness and audit uses to know which path of the CRITICAL-floor default was exercised.
- **Dimension**: FSR/Presentation
- **Location**:
  - `byroredux/src/boot/mod.rs`: `log::info!("Renderer upscaler selection: {}", renderer_config.upscaler)` immediately after `parse_renderer_config`.
  - `byroredux/src/main.rs` `install_universal_settings`: the `else if let Some(SettingValue::Choice(spec))` arm, `Ok(mode) => renderer_config.upscaler = mode`, which logs nothing.
  - `scripts/check-bench-determinism.sh` `run_once`: `selected_upscaler="$(sed -n 's/.*Renderer upscaler selection: //p' …)"`.
  - `byroredux/src/app_events.rs`: the `bench:` summary `println!`, which has no upscaler/extent token.
- **Status**: NEW. The same state was observed in `AUDIT_PERFORMANCE_2026-09-26.md` process notes ("Baseline selected the saved `render.upscaler = "fsr3/native-aa"`…") but never filed; no GitHub issue matches.
- **Description**:
  - `boot::run` parses the CLI and logs the selection at line ~337. `App::new` → `install_universal_settings` then loads `settings.toml` and, when `explicit_upscaler` is false, overwrites `renderer_config.upscaler` with the persisted choice, with no log.
  - The only trace of the real mode is the renderer's later `Frame extents: … (fsr3/native-aa)` / `Frame upscaler:` INFO lines, which contradict the boot line.
  - Precedence itself is intended (README §Player controls; `docs/engine/launcher.md` "Settings applied pre-device"). The defect is that the override is invisible and the log states something false.
- **Evidence**:
  - The live-run log shows `Renderer upscaler selection: fsr3/quality`, then `Frame extents: render=1280x720, output=1280x720 (fsr3/native-aa)`.
  - `~/.config/byroredux/settings.toml` contains `"render.upscaler" = "fsr3/native-aa"`, and `BYROREDUX_SETTINGS_PATH` is unset.
  - `FrameExtentSet::for_output` has no floor that could turn Quality into 1:1. The extents line prints `renderer_config.upscaler`, which confirms that the config value itself was native-aa.
  - None of these pass `--upscaler`, a settings path, or `BYROREDUX_SETTINGS_PATH`, so they all inherit the persisted value:
    - `scripts/check-bench-determinism.sh`
    - `scripts/renderer-eval.sh`, `renderer-eval-fnv.sh`
    - `scripts/bench-variability-envelope.sh`
    - `.claude/commands/audit-runtime/capture.sh` (`--game … --bench-frames … --bench-hold`)
    - most `docs/smoke-tests/*.sh`
  - `check-bench-determinism.sh` then stamps its per-run JSON with the pre-override `selected_upscaler`, next to render/output extents that disagree with it.
- **Impact**:
  - On any machine where someone once chose an upscaler in the pause menu, "no flag" no longer means FSR Quality.
  - Runtime-audit baselines, determinism manifests, eval captures and audit validation runs silently exercise a different reconstruction path: here native-aa, a render == output extent and a different jitter phase count / froxel grid / BLAS reservation.
  - Their logs claim Quality. Today's validation evidence for the "FSR default" path is affected.
- **Related**:
  - `.claude/commands/audit-fnv/SKILL.md`'s "the flag defaults to `fsr3`" premise.
  - `AUDIT_PERFORMANCE_2026-09-26.md` line ~114.
  - REN-D11-2026-09-27-02 (how such a value gets persisted).
- **Suggested Fix**:
  - Log the effective selection once, after `install_universal_settings`, naming its source (`cli` / `persisted settings.toml` / `default`). Move or re-emit the boot line so it is never pre-override.
  - Add an `upscaler=`/`render=`/`output=` token to the `bench:` line.
  - Have `check-bench-determinism.sh` and `capture.sh` either pass an explicit upscaler or point `BYROREDUX_SETTINGS_PATH` at a scratch file.


### LOW

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
