# #5470: TD3-2026-10-08-01: 48 string literals embed runs of 6–18 spaces from collapsed `\` line continuations; 8 are production text (CLI warning, console help, logs, an ESM error)

**Labels**: low,tech-debt,documentation,doc-rot
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5470

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-08.md` — `TD3-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments (user-visible text hygiene)
- **Location**: the 8 production sites:
  - `byroredux/src/cli_args.rs:101`: the warning printed when `--flag=value` is ignored.
  - `byroredux/src/commands/depth.rs:32`: the `depth.stats` help text shown by `help`.
  - `byroredux/src/scene/nif_loader.rs:1017`
  - `byroredux/src/systems/cinematic.rs:626,631`
  - `crates/bsa/src/archive/open.rs:445`
  - `crates/plugin/src/esm/reader.rs:946`: the decompression-bomb `ensure!` error.
  - `crates/renderer/src/vulkan/acceleration/blas_static.rs:562`: the BLAS-budget `warn!`.

  About 40 more are test assertion messages, for example:
  - `crates/renderer/src/vulkan/acceleration/tests/blas_static_tests.rs:358-383` (×5)
  - `crates/renderer/src/vulkan/bloom.rs:1472,1577,1595`
  - `byroredux/src/scheduler_access_tests.rs:1138`
  - `crates/ui/src/catalog.rs:733-734`
- **Status**: NEW. UI-D1-02 covers only `crates/ui/src/prepare.rs:79`, which is excluded from the counts.
- **Age**: 27 distinct commits from 2026-08-27 to 2026-10-07. The oldest group is `5d42e7226` (#3979, 7 sites). The newest
  are `63bf3347f` (#3817, 4 sites, 10-06) and `b7987d813` (10-07). It is an ongoing authoring pattern, not one bad commit.
- **Effort**: small (mechanical)
- **Description**:
  - A Rust `"…\⏎    …"` continuation strips the newline and the leading whitespace.
  - These literals were instead written, or re-flowed, as a single physical line with the continuation's indentation
    left inside the string.
  - The rendered text reads `stamped onto loaded              exterior cell roots`.
- **Evidence**:
  ```rust
  // byroredux/src/cli_args.rs:101
  "`{found}` was ignored — this CLI takes `{flag} {value}`              (space-separated); the `{flag}=value` form is not recognised"
  // byroredux/src/commands/depth.rs:32
  "Capture the depth buffer and report measured vs analytic depth resolution          (#3308); `depth.stats reversed` decodes a reversed-Z capture"
  ```
  Detection: `grep -rnE '"[^"]*[a-z0-9,;.)\`(-] {8,}[a-z(#\`+—-][^"]*"' --include='*.rs' crates byroredux tools`, then drop
  column-aligned help tables (`byro-dbg` `display.rs`, `nif_stats.rs`, `probe_form.rs`).
- **Impact**:
  - Cosmetic but user-facing: the CLI error, console help and warn-level logs.
  - It also defeats log grepping for the phrase.
  - No gate catches it, so it keeps recurring.
- **Related**: UI-D1-02 (same pattern, `prepare.rs:79`).
- **Suggested Fix**:
  - Re-wrap the 8 production literals with `\` continuations, then the test messages.
  - Add a `workspace_hygiene_tests` scan that flags a non-raw string literal containing `[^ ] {6,}[^ ]`. Exempt the help
    tables, or require them to be column-aligned with a leading two-space indent.

## Completeness Checks
- [ ] **SIBLING**: The ~40 test assertion messages with the same collapsed-continuation pattern swept too (UI-D1-02 owns `crates/ui/src/prepare.rs:79`)
- [ ] **TESTS**: A regression test pins this specific fix (a hygiene scan flagging `[^ ] {6,}[^ ]` inside non-raw literals)
