# SAFE-D4-2026-09-29-02: CI clippy is red on rustc 1.98.1 in `byroredux-sdk` and `byroredux-nif`, so the run aborts before the renderer is checked; `#![deny(clippy::undocumented_unsafe_blocks)]` is enforced nowhere in CI (regression of #4595)

**Labels**: medium,safety,renderer,tech-debt,bug

**Source report**: `docs/audits/AUDIT_SAFETY_2026-09-29.md`

**Regression of #4595** (#4595 is CLOSED; filed as a new issue rather than reopening it).

- **Severity**: MEDIUM. This is the same class as SAFE-D4-2026-09-21-01 (#4595): the only mechanical guard on the renderer's 733 `unsafe` blocks is off.
- **Dimension**: Unsafe-Block Discipline
- **Location**:
  - `.github/workflows/ci.yml:130` (`dtolnay/rust-toolchain@stable`, unpinned) and `:177-179` (`cargo clippy --workspace -- -D warnings`, no `--keep-going`);
  - the new-lint sites `crates/sdk/src/event.rs:447` and `crates/nif/src/{anim/controlled_block.rs:97, blocks/bs_geometry.rs:442/455/462, blocks/legacy_particle.rs:687, blocks/node.rs:1231, blocks/skin.rs:502, import/mesh/bs_tri_shape.rs:193, import/mesh/normal.rs:127, import/mesh/skin.rs:75, import/types.rs:1531}`.
- **Status**: Regression of #4595 (CLOSED 2026-09-22). #4595's remedy still holds: clippy runs even when tests are red. But the gate it restored no longer reaches the renderer. #4765 (CLOSED 2026-09-29, 554ef5c44) greened clippy on the local 1.96 toolchain only. No open issue covers this.
- **Description**:
  - CI installs `stable`, currently **rustc 1.98.1** (`48a229cea 2026-09-01`). The workstation runs 1.96.0, and the repo has no `rust-toolchain.toml`.
  - Clippy 1.98 adds `chunks_exact_to_as_chunks` and fires `question_mark` on one more pattern, giving 1 error in `byroredux-sdk` and 11 in `byroredux-nif`.
  - Cargo stops scheduling once those crates fail. The log shows `Compiling byroredux-renderer` (its build script) but never `Checking byroredux-renderer`, so the renderer's crate-level `#![deny(clippy::undocumented_unsafe_blocks)]` (`crates/renderer/src/lib.rs:21`) is never evaluated.
  - The clippy step has concluded `failure` on every main run in the sampled history, from `839b8dcea` (2026-09-25T14:08Z) through HEAD. The per-run causes before HEAD were not individually read; at HEAD, after #4765 landed, the only errors are the 12 rustc-1.98 lint sites below.
- **Evidence**: HEAD run `36609043348`, job `109545443996`, step "cargo clippy":
  ```
  error: using `chunks_exact` with a constant chunk size
     --> crates/sdk/src/event.rs:447:23
  error: could not compile `byroredux-sdk` (lib) due to 1 previous error
  error: this block may be rewritten with the `?` operator
     --> crates/nif/src/anim/controlled_block.rs:97:16
  … (10× chunks_exact_to_as_chunks in byroredux-nif)
  error: could not compile `byroredux-nif` (lib) due to 11 previous errors
  ```
  The same step printed `Checking` lines for 15 workspace crates (plus `Compiling` for the build scripts of fsr3-sys, cxx-bridge and renderer), and `byroredux-renderer` is never `Checking`.
- **Impact**: A comment-less `unsafe {}` added to the renderer today would pass CI. The local lint run above shows the renderer is clean at HEAD, so no such block exists yet. Every other clippy-enforced invariant in crates downstream of `sdk`/`nif` is also unchecked, and the job is permanently red, which trains readers to ignore it.
- **Related**: #4595, #4765, #4567 (the previous toolchain-bump red), SAFE-D4-2026-09-21-01. Memory note *Clippy --keep-going*: workspace clippy aborts at the first failing crate.
- **Suggested Fix**:
  - Fix the 12 sites. `as_chunks::<N>().0` is the suggested rewrite; `?` for `controlled_block.rs:97`.
  - Stop a toolchain bump from silently shadowing the renderer gate, by either:
    - pinning the CI toolchain with a `rust-toolchain.toml` that the workstation shares; or
    - adding `--keep-going` plus a dedicated `cargo clippy -p byroredux-renderer --no-deps -- -D clippy::undocumented_unsafe_blocks` step that cannot be pre-empted by an unrelated crate.

**Validated at HEAD 9fcfdc3fc**: HEAD CI run 36609043348 job "Test + Check + Clippy" concluded `failure`; its log shows the 1 `byroredux-sdk` error (`crates/sdk/src/event.rs:447`) and 11 `byroredux-nif` errors at the listed sites; `.github/workflows/ci.yml` uses `dtolnay/rust-toolchain@stable` and `cargo clippy --workspace -- -D warnings` without `--keep-going`; no `rust-toolchain.toml` exists in the repo.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
