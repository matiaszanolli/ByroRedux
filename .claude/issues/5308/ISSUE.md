# #5308: TD8-2026-10-05-01: CI `Test + Check + Clippy` has been red on every push since 2026-10-01 — rustc 1.99's `chunks_exact_to_as_chunks` lint (24 sites) plus two debug-ui float-fallback errors; dependents are never linted

Labels: medium,tech-debt,bug
Filed from: docs/audits/AUDIT_TECH_DEBT_2026-10-05.md

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-05.md` (TD8-2026-10-05-01) · audit HEAD `a2c24b16e`

- **Severity**: MEDIUM.
  - It is promoted because a red board has hidden real signal for 4 days and 40+ pushes, and this is the
    second toolchain-drift red in a row (#5121).
  - The gate also stops linting every crate downstream of the failures.
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft (clippy gate)
- **Location**: `.github/workflows/ci.yml:138` (`dtolnay/rust-toolchain@stable`), the clippy step
  `cargo clippy --workspace --keep-going -- -D warnings`. The failing sites:
  - **plugin (17)**:
    - `esm/cell/helpers.rs:40,112`
    - `esm/cell/walkers.rs:387`
    - `esm/cell/wrld.rs:554`
    - `esm/records/actor/mod.rs:1476,1482,1535`
    - `esm/records/load_screen.rs:199`
    - `esm/records/misc/imagespace.rs:157,161`
    - `esm/records/misc/water.rs:1464`
    - `esm/records/misc/world.rs:636,1448`
    - `esm/records/outfit.rs:80`
    - `esm/records/pathgrid.rs:94,109`
    - `esm/records/tree.rs:190`
  - **renderer (4)**:
    - `vulkan/context/depth_capture.rs:100`
    - `vulkan/context/screenshot.rs:91`
    - `vulkan/groundcover/frame.rs:183`
    - `vulkan/pipeline.rs:25`
  - **hkx (2)**: `animation.rs:503,722`
  - **menuxml (1)**: `raster.rs:81` (`chunks_exact_mut`)
  - **debug-ui (2)**: rustc future-incompat "falling back to `f32` as the trait bound `f32: From<f64>` is not
    satisfied" at `panels.rs:170,471`, an error under `-D warnings`
- **Status**: NEW. It is the same class as CLOSED #5121, which covered rustc 1.98.1 in sdk and nif.
  `AUDIT_SAFETY_2026-10-05` §"Other existing items" mentions the red as "lint drift" but files nothing.
- **Age**: rustc 1.99.0 was released 2026-09-28. The first red run on it was `36908954333` (`4ad847a81`,
  2026-10-01 18:43Z). Every `ci.yml` push since then is red; I sampled 40 of them through HEAD run
  `37344961054`.
- **Effort**: small
- **Description**:
  - The clippy job installs whatever `stable` is on the day. The repository pins no toolchain, and the only one
    installed locally is 1.96 (`rustup toolchain list`: stable = 1.96.0).
  - On 1.96, `cargo clippy --workspace --keep-going -- -D warnings` and the `--all-targets` variant both exit 0
    here. Nobody can reproduce the CI failure locally.
  - Because the lint is an error in five library crates, `--keep-going` builds no metadata for them. Every
    crate that depends on them goes unlinted on CI: the `byroredux` binary, sdk, scripting, the debug server and
    the tools.
  - Behind the first layer there is already a second one. On 1.99 the `cargo check` steps of the same job warn
    `unused import: super::builders::*` at `byroredux/src/cornell/{glass_dragon,godray_lab,oracle}.rs:4`. Those
    warnings become clippy errors once the libraries are green.
- **Evidence**:
  - Run `37344961054`, job `111881105065`:
    ```
    rustc 1.99.0 (b940084d7 2026-09-28)
    error: using `chunks_exact` with a constant chunk size   --> crates/plugin/src/esm/records/pathgrid.rs:94:39
      = help: ... #chunks_exact_to_as_chunks
    error: could not compile `byroredux-menuxml` (lib) due to 1 previous error
    error: could not compile `byroredux-hkx` (lib) due to 2 previous errors
    error: could not compile `byroredux-debug-ui` (lib) due to 2 previous errors
    error: could not compile `byroredux-plugin` (lib) due to 17 previous errors
    error: could not compile `byroredux-renderer` (lib) due to 4 previous errors
    ```
  - Job history for `Test + Check + Clippy`:
    - failure on every run from 09-30 15:42Z through HEAD;
    - 09-30 was #5121's 1.98.1 layer: sdk and nif, plus an example compile;
    - 1.99 from 10-01 18:43Z.
- **Impact**:
  - Four days of pushes, including the whole 10-01 → 10-04 fix wave, got no CI lint signal for the binary
    crate.
  - A permanently red board teaches readers to ignore it. It is red alongside ABBA (ECS-2026-10-05-D1-01) and
    Vulkan validation (CONC-D3-2026-10-05-01).
  - The renderer-only `undocumented_unsafe_blocks` step still runs independently (#5121's fix), so the safety
    gate is unaffected.
- **Related**: #5121 (CLOSED, previous layer), #3894/#4765 (toolchain-drift class), SAFETY-2026-10-05 §"Other
  existing items", ECS-2026-10-05-D1-01, CONC-D3-2026-10-05-01.
- **Suggested Fix**:
  - Mechanically rewrite the 24 sites to `as_chunks::<N>().0` (the lint's own suggestion). Give the two debug-ui
    literals an explicit `f32` suffix. Delete the three unused `builders::*` imports.
  - Then stop the recurrence. Either commit a `rust-toolchain.toml` and bump it deliberately, or add a
    `continue-on-error` clippy job on `beta` so a new lint shows as a warning a release early.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
