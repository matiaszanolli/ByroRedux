# TD3-2026-10-15-13: CLAUDE.md / contributing.md toolchain note ("default distro rustc here is 1.93.1 ... `rustup which` workaround required") no longer matches this machine

Labels: low,dependencies,tech-debt,documentation,doc-rot

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD3-2026-10-15-13`).

- **Severity**: LOW (environment-dependent; needs the user's confirmation)
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `CLAUDE.md:29-40`; `docs/contributing.md:123-145`; `rust-toolchain.toml:9-11` comment
- **Status**: NEW
- **Age**: `8529a3e6e` (2026-10-01); `rust-toolchain.toml` pin #5308
- **Effort**: trivial
- **Description**: in this session `which cargo` is `~/.cargo/bin/cargo`, `cargo --version` = 1.96.0 (the repo pin, active in the repo dir; `rustup toolchain list` shows `1.96.0 (active)`), and the distro `/usr/bin/rustc` is 1.97.1, not 1.93.1 — both satisfy the 1.96 MSRV. The "distro cargo shadows the rustup shims; `cargo +1.96.0` does NOT work" premise does not hold on this PATH, so bare `cargo test -p byroredux --bin byroredux` may already work. I did not run it (read-only audit).
- **Evidence**: `rustup toolchain list`, `cargo --version`, `/usr/bin/rustc --version`.
- **Impact**: agents follow the longer `rustup which` ritual (still correct) believing the short form fails; low harm.
- **Related**: #4466, #5308.
- **Suggested Fix**: user to confirm on their interactive shell; if bare `cargo` works, reword to "the repo pins 1.96.0 via rust-toolchain.toml; if your PATH puts a distro cargo first, use the `rustup which` form".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
