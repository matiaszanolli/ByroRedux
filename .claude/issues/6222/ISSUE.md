# #6222: REG-2026-10-21-01: Regression of #5849 — The blocking clippy gate is red again, and it hides the bin crate's lints

**Labels**: medium,esm-plugin,tech-debt,bug

**Source**: docs/audits/AUDIT_REGRESSION_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: CI gates / closed-fix regression · **Location**: `crates/plugin/src/esm/records/weather.rs:808`, `crates/plugin/src/esm/reader.rs:621`, `crates/plugin/src/esm/cell/index.rs:714-716` (2 lints). The hidden ones are at `byroredux/src/asset_provider/object_graph.rs:260`, `byroredux/src/cell_loader/load.rs:676`, …

- **Severity**: MEDIUM
- **Dimension**: CI gates / closed-fix regression
- **Location**: `crates/plugin/src/esm/records/weather.rs:808`, `crates/plugin/src/esm/reader.rs:621`, `crates/plugin/src/esm/cell/index.rs:714-716` (2 lints). The hidden ones are at `byroredux/src/asset_provider/object_graph.rs:260`, `byroredux/src/cell_loader/load.rs:676`, `byroredux/src/cell_loader/spawn.rs:403`, `byroredux/src/commands/terrain.rs:17`, `byroredux/src/scene/nif_loader.rs:519` and `byroredux/src/terrain_height.rs:47`. Gate: `.github/workflows/ci.yml` step `cargo clippy` (`cargo clippy --workspace --keep-going -- -D warnings`).
- **Status**: Regression of #5849. #5849 ("The blocking clippy gate is red again … leaves … the `byroredux` bin unlinted") was closed by `251d491ef` on 10-15. The gate went red again on 10-19, and `12677d718` re-zeroed it on 10-20 without an issue. It was green for three runs, then `3ab1b5557` turned it red again. The open #6193 covers only the non-blocking `--all-targets` step, and #5622 covers allow-reason comments. Neither covers this step.
- **Description**: In CI run 39645304183 (HEAD `4a3e89d56`), the plugin lib fails with 4 deny-level lints:
  - `nonminimal_bool` ×2 at `index.rs:714`/`:715`, from `765e387a0` (Fix #6084)
  - `redundant_closure` at `reader.rs:621` (`.filter(|slot| is_kind(slot))`), from `0a2955d33` (Fix #6118)
  - `manual_is_multiple_of` at `weather.rs:808`, from `67433ee3a` (Fix #6038)

  `--keep-going` keeps independent crates going, but nothing that depends on `byroredux-plugin` is linted. The CI log's `Checking` list is missing `byroredux`, `byroredux-scripting`, `byroredux-game-detect`, `byro-detect` and `byro-launcher`. Run locally on the same toolchain without `-D`, clippy reports 6 more lints in the bin, all from 10-20/10-21 commits:
  - `nonminimal_bool` at `object_graph.rs:260` (Fix #6126 `9a1f24b3e`)
  - `needless_borrow` at `load.rs:676` (`&absorbed` after Fix #6127 `67e724c3e` made it a `&mut`)
  - `too_many_arguments` 8/7 at `spawn.rs:403` (`c53d87679`, part of #6048, 10-21)
  - `unnecessary_lazy_evaluations` at `commands/terrain.rs:17` and `type_complexity` at `terrain_height.rs:47` (both `5017a64c2`)
  - `type_complexity` at `nif_loader.rs:519` (Fix #6124 `8879b67b4`)

  `byroredux-scripting` is clean.
- **Evidence**: The failing step is the only failure in the `Test + Check + Clippy` job of all 17 runs from `3ab1b5557` to `4a3e89d56`, and `cargo test` passes in each. The last green run is `79477b4a3`, and the first red one is `3ab1b5557`, whose range `79477b4a3..3ab1b5557` contains `67433ee3a`. Local `cargo clippy --workspace --keep-going --message-format=short` reports `byroredux-plugin (lib) generated 4 warnings` and `byroredux (bin "byroredux") generated 6 warnings` (`/tmp/audit/regression/clippy_warn.txt`).
- **Impact**: For 27 hours `main` has had no working lint gate on the crate that holds most of the gameplay code. Because the whole `Test + Check + Clippy` job is red, a real `cargo test` failure would show up as the same red job. This is the fifth blocking-gate regression this month (#5308, #5477, #5621, #5849, and now this), and each one went in through fix commits that nobody ran clippy on.
- **Related**: #5849, #5621, #5308, #6193 (open, `--all-targets`), `12677d718` (the 10-20 re-zero)
- **Suggested Fix**: Apply the 10 mechanical fixes. Clippy prints the replacement for each, and the `too_many_arguments` site needs either a parameter struct or a reasoned `#[allow]`, per #5622. Consider running `cargo clippy -p <touched crates> -- -D warnings` in the fix-issue flow before commit. The gate only fails after the push.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **GATE**: `cargo clippy --workspace --keep-going -- -D warnings` exits 0 locally on the pinned 1.96.0 toolchain, and the CI `Checking` list includes `byroredux`, `byroredux-scripting`, `byroredux-game-detect`, `byro-detect`, `byro-launcher`

_Source: `docs/audits/AUDIT_REGRESSION_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
