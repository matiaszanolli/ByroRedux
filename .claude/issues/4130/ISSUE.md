### REG-02: `cargo clippy --workspace --all-targets -- -D warnings` is red at HEAD on `field_reassign_with_default`, a lint #4090 never covered

- **Severity**: LOW
- **Dimension**: tech-debt / CI-gate
- **Location**: test code across `crates/core` (e.g. `crates/core/src/ecs/resources/ownership_tests.rs:307,336,386,445`), `crates/core/src/stealth.rs:797` (a `doc_lazy_continuation` hit in the same strict run), `crates/renderer`, and the `byroredux` binary
- **Status**: NEW
- **Description**: #4090's fix (`da3d5f22`) made the *documented* clippy invocations green: `cargo clippy --workspace -- -D warnings` (core's actual CI command) and `cargo clippy -p byroredux-renderer --all-targets` (without `-D warnings`). Both are still green. However, the *combined* stricter invocation `cargo clippy --workspace --all-targets -- -D warnings` fails today on `clippy::field_reassign_with_default` (plus a `clippy::doc_lazy_continuation` hit), lints #4090's fix never addressed. This was never claimed fixed by #4090's commit and is not a regression of it — it's a pre-existing gap in test code that no CI job currently exercises with that exact flag combination.
- **Evidence**: Re-ran `cargo clippy --workspace --all-targets -- -D warnings` directly this session — confirmed still red with `error: field assignment outside of initializer for an instance created with Default::default()` at `crates/core/src/ecs/resources/ownership_tests.rs:307,336,386,445` (constructing `OwnershipSnapshot` then reassigning fields one at a time instead of using struct-update syntax), plus `error: doc list item without indentation` at `crates/core/src/stealth.rs:797` (`clippy::doc_lazy_continuation`), while the two commands #4090's fix actually targets remain clean.
- **Impact**: Low — cosmetic lint in test code, not a correctness issue. But it is the same class of gap the project's own `CLAUDE.md` calls out for `cargo test -p byroredux-core` (a documented command that silently omits coverage): here a plausible "make CI stricter" step would immediately go red on unrelated findings, encouraging either a rushed mass-fix or reverting the stricter flag.
- **Related**: #4090 (adjacent, not overlapping scope).
- **Suggested Fix**: Fix the enumerated `field_reassign_with_default` sites in `ownership_tests.rs` (use struct-update syntax, e.g. `OwnershipSnapshot { physics_bodies: 1, ..Default::default() }`) and the `doc_lazy_continuation` indentation in `stealth.rs:797`, then either adopt `--all-targets -- -D warnings` as the actual CI command or explicitly document why CI intentionally excludes `--all-targets`.

## Completeness Checks
- [ ] **TESTS**: `cargo clippy --workspace --all-targets -- -D warnings` re-run clean after the fix
