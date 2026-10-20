# TD8-2026-10-19-03: `vulkan::material::presets` (five Disney presets, ~200 lines with tests) has had no consumer since it landed

Labels: low,renderer,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW — **Dimension**: 8 — **Status**: NEW
- **Location**: `crates/renderer/src/vulkan/material.rs:~854-1060` (`polished_metal` `:867`, `car_paint` `:911`, `lacquered_plastic` `:926`, `painted_matte` `:942`, `skin_wax_marble` `:958`; private `#[cfg(test)] mod tests` `:973`)
- **Age**: c09d63a6f (2026-05-23, #1251)
- **Effort**: trivial
- **Description**: `git grep -n 'presets::'` finds only the unrelated `byroredux_settings_io::presets`; every call to the five fns is inside material.rs's own test module. Doc justifies it as the citable reference table (`feedback_no_guessing`), but nothing — including the Cornell `mat.*` harness — builds a `GpuMaterial` from them.
- **Evidence**: `git grep -nw -e polished_metal -e car_paint -e lacquered_plastic -e painted_matte -e skin_wax_marble` → material.rs only.
- **Proposed fix**: either have the Cornell/`mat.*` harness use the presets (gives them a consumer) or delete the module and keep the Hyperion numbers in the doc comment where they are citable.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
