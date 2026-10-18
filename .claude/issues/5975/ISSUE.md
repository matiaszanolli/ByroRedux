# SCR-D1-2026-10-17-07: Placeholder accounting - `AddAchievement` records into a store nothing reads, and `is_placeholder` ignores nested effects

Issue: #5975
Labels: low, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D1-2026-10-17-07: Placeholder accounting - `AddAchievement` records into a store nothing reads, and `is_placeholder` ignores nested effects

- **Severity**: LOW
- **Dimension**: Recognizer Chain
- **Location**: `crates/scripting/src/translate/effects/model.rs:1014-1019`, `crates/scripting/src/misc_stats.rs:64-82`, `crates/scripting/examples/fragment_coverage.rs:421`, `mq101_conformance.rs:1744`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: (a) `Effect::AddAchievement` only does `Achievements::award(id)`; `Achievements::has` has no production reader (only `fragment/vm/tests.rs`), and `docs/engine/scripting.md:738-740` says the platform profile is the host's. By the skill's rule ("any new effect whose runtime arm only records a value needs listing in `is_placeholder` and a pin") it belongs in `is_placeholder` and `only_the_counter_only_primitives_are_placeholders`. (b) `is_placeholder` is a flat `matches!`; the harnesses call `effects.iter().any(Effect::is_placeholder)` on the top level only, so a `ShowRaceMenu`/`RequestSave`/`SetHudCartMode` inside an `Effect::Conditional` (If/ElseIf, new in M47.2), `QuestScope` or similar is not reported as a placeholder-claimed fragment; the `kind()` histogram likewise counts the wrapper, not its contents.
- **Evidence**: `rg` for `Achievements` readers: only `misc_stats.rs`, `registration.rs`, `deferred.rs`, save registry; `rg is_placeholder`: model + two examples + the pin test.
- **Impact**: Coverage numbers overstate behavioural coverage (no gameplay effect).
- **Related**: #4328, #4372.
- **Suggested Fix**: List `AddAchievement` (and extend the pin test); make the harness check recurse through `Conditional`/`QuestScope` (or give `Effect` a `contains_placeholder`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
