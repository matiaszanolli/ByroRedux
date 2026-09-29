# TD2-2026-09-29-01: Every generated shader constant is hand-typed three times

**Labels**: low,renderer,shaders,tech-debt,bug

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 2 · **Status**: NEW · **Effort**: medium (mechanical)
- **Location**:
  - `crates/renderer/build.rs:39-1454`: `main()` is one 1415-line function made of 315 `writeln!` calls
    (+105 lines this window).
  - `crates/renderer/src/shader_constants_data.rs`: 369 `pub const`s.
  - `crates/renderer/src/shader_constants.rs:888` `generated_header_contains_all_defines`: a hand
    pin-list of `(name, format!("#define NAME {NAME}u"))` tuples.
- **Description**:
  - One new GLSL constant takes three edits: declare it, hand-write its emitter (choosing the `u` suffix
    or the `{:?}` float format by hand), and add a matching pin tuple.
  - The pin re-types the same format string, so a wrong suffix typed twice passes.
  - The provenance gate stops shaders from redeclaring constants, but nothing derives the emitter from
    the data.
- **Suggested Fix**:
  - Declare `SHADER_DEFINES: &[(&str, ShaderValue)]` in `shader_constants_data.rs`, with variants
    `Uint/Int/Float/Vec3/Raw` and a `Section` variant for the grouping comments.
  - build.rs iterates it to write the header.
  - The test asserts that each entry renders into the header, and that `shader_constant_data_names()`
    equals the table.

**Validated at HEAD 9fcfdc3fc**: `crates/renderer/build.rs` `main` is 1416 lines starting at :39 with 315 `writeln!` calls; `shader_constants_data.rs` has 366 `pub const` lines; `generated_header_contains_all_defines` (`shader_constants.rs:888`) is a hand-written `(name, format!(...))` pin list.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
