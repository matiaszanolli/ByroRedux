# #7014 — PHYS-D2-03: #6689 removed the only guard that tied each profile accessor to its own environment variable

**Labels**: low,physics,test-gap,bug · **Filed from**: `docs/audits/AUDIT_PHYSICS_2026-11-15.md` (2026-11-15)

- **Severity**: LOW
- **Dimension**: Dim 2 (guard coverage, `8124d9594`)
- **Location**: `crates/physics/src/sync.rs:250-272`, tests at `crates/physics/src/sync.rs:2625-2680`
- **Trigger Conditions**: A copy-paste edit of `profile_fallers` (e.g. `env_flag_once(&PROFILE_FALLERS, PROFILE_VAR)`, or sharing one `OnceLock`).
- **Status**: NEW (test gap only; the production code is correct)
- **Description**: The deleted `profile_accessors_read_their_own_variables` asserted `var_os("BYRO_PROFILE")` and `var_os("BYRO_PROFILE_FALLERS")` each appear in production text. The replacement `the_two_profile_flags_name_different_variables` only compares the two `const`s to string literals and calls each accessor twice, so it cannot see which constant or cell an accessor passes. `an_env_flag_is_read_once_and_then_cached` exercises `env_flag_once` in isolation. The caching guard (`profile_flags_are_read_once_per_process_not_per_tick`) is not vacuous. It scans the `physics_sync_system` body through whole-file `include_str!` and `find("pub fn physics_sync_system")` hits the production fn before the test module. I checked the Methodology rule here: no guard in this commit became vacuous, but this wiring check was dropped.
- **Impact**: A wiring mistake would make `BYRO_PROFILE_FALLERS` inert or tied to `BYRO_PROFILE`. It is a diagnostic flag, so the cost is a misleading profile run.
- **Related**: #6689, #2881.
- **Suggested Fix**: Make each accessor a thin wrapper over a single `static` + `const` pair (e.g. a small macro), or assert on `production_text` that `profile_phases` contains `PROFILE_VAR` and `profile_fallers` contains `PROFILE_FALLERS_VAR`.

Source: `docs/audits/AUDIT_PHYSICS_2026-11-15.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
