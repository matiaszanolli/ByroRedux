# #5478: TOOL-CI-2026-10-08-02: `dtolnay/rust-toolchain@stable` survives in 10 CI steps after the 1.96.0 pin, and its version output names a toolchain no step uses

**Labels**: low,tech-debt,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5478

**Source**: `docs/audits/AUDIT_TOOLING_2026-10-08.md` — `TOOL-CI-2026-10-08-02` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: Tool CLIs / CI gate (`.github/`, `rust-toolchain.toml` → tech-debt)
- **Exposure**: developers only (CI log readers; time on the self-hosted runners)
- **Location**:
  - `.github/workflows/ci.yml:138,222,317,354`
  - `.github/workflows/real-data-gates.yml:112,161` (corpus, parsers)
  - `.github/workflows/rt-correctness.yml:28,72`
  - `.github/workflows/playable-smoke.yml:38`
  - `rust-toolchain.toml`
- **Status**: NEW. The parsers audit routed it here. The intent is #5308, which is closed.
- **Description**: #5308 pinned `channel = "1.96.0"` (plus clippy) in `rust-toolchain.toml` to stop surprise reds when stable moves. The workflows still run `dtolnay/rust-toolchain@stable` first. That step installs current stable, sets it as rustup's *default*, and reports its version and cache key. Inside the checkout, rustup's toolchain-file override wins, so every `cargo` call runs on 1.96.0. The pin works. The step is dead weight that looks authoritative. `ci.yml:289-290` already documents this override for the Miri job.
- **Evidence**: CI run 37848932617, Test+Check+Clippy:
  - dtolnay step: `stable-x86_64-unknown-linux-gnu unchanged - rustc 1.99.0`, then `info: note that the toolchain '1.96.0-x86_64-unknown-linux-gnu' is currently in use (overridden by '…/rust-toolchain.toml')`. Its version step prints `rustc 1.99.0`.
  - Swatinem/rust-cache keys on `1.96.0 x86_64-unknown-linux-gnu ac68faa20…`.
- **Impact**: No effect on correctness: builds, tests, clippy and the cache key all use 1.96.0. Two costs remain. A log reader, or the parsers audit as happened here, concludes the lane is unpinned. Each self-hosted game-data runner (`corpus`, `parsers`, `playable-smoke`) also installs and keeps updating a stable toolchain it never uses. Someone who deletes `rust-toolchain.toml` believing CI pins elsewhere would silently re-open #5308.
- **Related**: #5308, `4a6a6cdd7`, `10f02bac1`
- **Suggested Fix**: Do one of the following, and drop the now-redundant `components: clippy` input:
  - Replace the step with `dtolnay/rust-toolchain@1.96.0` (or `@master` with `toolchain: 1.96.0`).
  - Remove it and rely on the toolchain file, adding a `rustc --version` step so the log names the toolchain actually used.

## Completeness Checks
- [ ] **SIBLING**: All 10 `dtolnay/rust-toolchain@stable` steps across `ci.yml`, `real-data-gates.yml`, `rt-correctness.yml`, `playable-smoke.yml` changed together (the Miri `@nightly` step is intentionally different)
