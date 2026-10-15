# TD1-2026-10-15-10: `crates/renderer/src/shader_constants_data.rs` is 2900 LOC, 51% comments, and spells each of ~440 constants twice (data table — do not split)

Labels: low,renderer,shaders,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD1-2026-10-15-10`).

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `crates/renderer/src/shader_constants_data.rs` (consts L1–2255, generator machinery `ShaderValue`/`HeaderLine`/`render_shader_constants_header` L2256–2380, `SHADER_DEFINES` table L2381–2900)
- **Status**: NEW (carried as "data table" in the baseline; first time it has a finding)
- **Age**: +131 lines since 10-12 from the grass commits (`b731b01f5`, `39c9e39f0`, `af8323881`, `2c86473ff`, `3f1c5e962`)
- **Effort**: small
- **Description**: the skill's rule for data tables is a static table or generation, not a split; the file is already that, and it is `include!`d by both `shader_constants.rs` and `build.rs`, which would make a file split fragile. What it still carries: (1) each constant is a `pub const X` (≈440) and again a `HeaderLine::Define("X", ShaderValue::…(X))` row in `SHADER_DEFINES` (~520 lines), so a new constant is two edits in two distant places and a missed row means a Rust value with no GLSL `#define` (the lockstep class `feedback_shader_struct_sync` warns about); (2) the generator types and renderer (L2256–2380) live in the data file only because `build.rs` can `include!` one file.
- **Evidence**: `grep -cE '^\s*(pub )?const ' → 440`; comment lines 1485 of 2900; `SHADER_DEFINES` rows at L2381.
- **Impact**: this is the #3815 / #3882 provenance gate's subject; a missed row is silent unless the gate's tests catch it.
- **Related**: #3815, #3882, #3880 (closed).
- **Suggested Fix**: a `shader_const!{ NAME: u32 = …, "section comment" }` macro (or a `build.rs` pass) that emits both the `const` and its `SHADER_DEFINES` row from one line; move the generator types to a second `include!`d file. Do not split by shader domain.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
