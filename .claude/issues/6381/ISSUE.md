# TD3-2026-10-25-10: the symbol validator cannot see scripts, native sources or lockfiles, producing about 35 of the 371 advisories

Labels: low, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6381
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW (tooling noise; the advisory list is meant to be cleared)
- **Dimension**: 3
- **Effort**: small
- **Evidence**: of the 371, 35 symbols exist in tracked non-`.rs` files: env vars in `scripts/renderer-eval*.sh` (`BYROREDUX_RENDER_EVAL_*` x5, `BYROREDUX_GROUNDCOVER_EVAL_CASES`, `BYROREDUX_RENDER_EVAL_RUNNER`), FSR native names in `crates/fsr3-sys/native/byro_fsr3.cpp` (`ffxCreateContext`, `ffxQuery`, `enableSharpening`, `jitterOffset`, `preExposure`, `motionVectorScale`, `viewSpaceToMetersFactor`), crate names in `Cargo.lock` (`cranelift`, `enumset`, `realfft`, `ruffle_render`, `xxhash64`), and bench telemetry labels in `.tsv`/scripts (`gpu_composite`, `gpu_sky_cube`, `gpu_cloud_shadow`, ...). Another ~220 are vanilla EditorIDs / ObScript+Papyrus names / GMST names / NIF texture names (intentional, see triage).
- **Fix**: Let `_audit-validate.sh` widen the haystack to `scripts/**`, `*.sh`, `*.cpp`, `*.glsl`, `Cargo.lock`. Add an allowlist file for record-name and EditorID docs (`charal-*`, `scripting.md`, `dialogue-trees.md`, `npc-spawn-ai-packages.md` EditorIDs) so the remaining ~100 are the actionable set.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD3-2026-10-25-10`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
