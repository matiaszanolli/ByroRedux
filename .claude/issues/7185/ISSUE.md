# #7185 — TOOL-D4-2026-11-21-01: the renderer-option reader silently ignores `--flag=value`

Labels: low,tech-debt,bug

- **Severity**: LOW
- **Dimension**: Boot Handoff & Persistence
- **Exposure**: a developer or a capture-script author
- **Location**: `byroredux/src/cli_args.rs` `parse_renderer_config` (its `option` closure)
- **Status**: NEW
- **Description**: the closure looks the flag up by exact match and returns `Ok(None)` on a miss. `--upscaler=taa`, `--fsr-quality=…`, `--tonemap=…` and every `--rt-test-*` value flag are therefore dropped with no diagnostic: `--upscaler=taa` silently runs FSR. `parse_string_arg` warns on the equals form for exactly this reason (`the_equals_form_is_reported_rather_than_silently_dropped`).
- **Evidence**: coordinator re-read `cli_args.rs:150-158`.
- **Suggested Fix**: on the `None` path, report the equals form (`equals_form_diagnostic`) as a warning or an error, with one test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other writers / readers / folds named above)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_TOOLING_2026-11-21.md`
