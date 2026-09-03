# #3741 — TD2-2026-08-30-01: test_paths.rs was pub(crate), so parse_real_esm.rs re-hardcoded 42 Steam roots

**Severity**: MEDIUM · **Location**: `crates/plugin/src/esm/test_paths.rs`, `crates/plugin/src/esm/mod.rs`, `crates/plugin/tests/parse_real_esm.rs`
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-30.md` (TD2-2026-08-30-01)

`test_paths.rs` (#1058) centralizes the `BYROREDUX_<GAME>_DATA` env-var
override shape for real-data integration tests, but was declared `#[cfg(test)]
pub(crate) mod test_paths;`. An integration test under `tests/` links the
crate as a *normal* (non-test) dependency, so it structurally cannot reach a
`pub(crate)` item, nor one gated on `#[cfg(test)]` (that gate only applies
within the crate's own `cargo test` compilation unit) — `pub(crate)` alone
wasn't the whole story. `crates/plugin/tests/parse_real_esm.rs` re-hardcoded
the same Steam roots 42 times instead, and diverged while doing it: it used
`BYROREDUX_OBL_DATA` where `test_paths.rs` used `BYROREDUX_OBLIVION_DATA`
for the identical game — a real, confirmed instance (not just a theoretical
risk) of the exact "env var not consulted the same way everywhere" failure
#1058 set out to remove. It also used `BYROREDUX_FO76_DATA`, an env var
`test_paths.rs` had no accessor for at all despite its own module doc
listing FO76 among the covered games.

## Fix implemented

Per the issue's own cheaper option 2 (scoped to `crates/plugin` — the 134
workspace-wide occurrences across 7 crates are the larger option-1 "medium
effort" `crates/test-paths` dev-dependency crate, not attempted here):

- `test_paths.rs`: every accessor `pub(crate) fn` → `pub fn`; added the
  missing `fo76_data_dir()` (closing the module-doc-vs-implementation gap);
  refactored each game's `(env_var, default)` pair into `pub const
  <GAME>_ENV`/`<GAME>_DEFAULT` string constants, so both function-call sites
  and `const`-context sites (a `RosterCase` struct array, several tuple
  arrays) can reference the same literals without re-typing them.
- `esm/mod.rs`: `#[cfg(test)] pub(crate) mod test_paths;` → `pub mod
  test_paths;` (unconditional — the module has no test-only dependencies, so
  compiling it unconditionally costs nothing).
- `parse_real_esm.rs`: all 84 literal occurrences (42 env-var strings + 42
  path strings) replaced with `test_paths::<GAME>_ENV`/`<GAME>_DEFAULT`
  references. `BYROREDUX_OBL_DATA` is now `BYROREDUX_OBLIVION_DATA`
  everywhere — the divergence is closed, not preserved. The file's own
  `data_dir(env_var, fallback) -> Option<PathBuf>` wrapper (existence-checked,
  skip-on-miss — a slightly more defensive shape than the bare
  `*_data_dir()` accessors) is unchanged; only its call sites' literal
  arguments changed.

**SIBLING** (issue's own checklist item): the 134-occurrence, 7-crate figure
is real but out of scope for this fix — it's the larger option-1 effort the
issue itself marks "medium" (a new `crates/test-paths` dev-dependency crate),
distinct from the "small" fix implemented here. Not attempted in this pass;
the remaining 6 crates (`bsa`, `nif`, `audio`, `facegen`, `spt`, `sfmaterial`,
`byroredux/tests`) keep their own per-file helpers as `test_paths.rs`'s own
doc already documented as an accepted scope boundary from #1058.

**TESTS** (issue's own checklist item): two new tests in `test_paths.rs` —
`every_env_name_follows_the_documented_shape` pins the `BYROREDUX_<GAME>_DATA`
naming convention for all 7 games (guards against a future divergent name
like the `BYROREDUX_OBL_DATA` this fix just closed), and
`data_dir_accessors_fall_back_to_their_documented_default_when_unset` pins
each accessor's default-path fallback. Also ran the real integration suite
directly against the mounted game data (`cargo test -p byroredux-plugin
--test parse_real_esm -- --ignored`) — all 24 tests pass with the
newly-unified paths, confirming the fix works functionally, not just
compiles.

Full workspace: `cargo test --no-fail-fast` 7062 passing, 0 failing (+2 new
tests).
