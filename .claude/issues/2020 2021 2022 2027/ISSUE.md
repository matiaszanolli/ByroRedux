# Issue batch: 2020, 2021, 2022, 2027

## #2020 — SAVE-D1-NEW-02: restore_world's release-mode insert_batch bound-check gap is real but currently dormant
- Severity: LOW (dormant — no reachable production path) (bug, ecs)
- Location: `crates/core/src/ecs/world.rs:204-209` (`insert_batch`'s `debug_assert`, compiled out under `--release`); `crates/save/src/driver.rs:78-108` (`restore_world`)
- `insert_batch`'s `entity < next_entity` guard is `debug_assert`-only, compiled out in release. All non-test call sites of `restore_world` are inside `#[cfg(test)]` — live `load` path uses `restore_resources` + `apply_deltas` instead, unaffected.
- Suggested fix: if/when `restore_world` gains a live call site, promote the check to a real `Result`-returning validation.

## #2021 — SAVE-D2-04: LightSource / LightFlicker have no dedicated save/load round-trip test
- Severity: LOW (bug, ecs)
- Location: `crates/core/src/ecs/components/light.rs`; registered at `byroredux/src/save_io.rs:181-182`
- Both types are registered + delta-columned but no test round-trips either. Flat structs (low risk) but a serde regression would only surface as a runtime visual bug.
- Suggested fix: add an assertion-bearing round trip, piggyback on `binary_registry_round_trips_including_scripttimer`.
- Completeness check: SIBLING sweep for other under-tested flat registered components while adding this test.

## #2022 — SAVE-D2-06: ItemInstancePool has no round-trip test but currently holds no real data
- Severity: LOW (informational — test-coverage note, not a live risk) (bug, ecs)
- Location: `crates/core/src/ecs/resources/mod.rs:689-693,707-709`; registered at `byroredux/src/save_io.rs:193`
- `ItemInstance` is currently a placeholder (`_reserved: ()`) — the doc comment's round-trip claim is vacuously true today.
- Suggested fix: no action required now; add a round-trip test in the same commit that gives `ItemInstance` real fields. (Tracked so it isn't forgotten.)

## #2027 — SCR-D1-NEW-01: Four of six PexError variants have zero test coverage anywhere in the repo
- Severity: LOW, untrusted-input (bug, tech-debt)
- Location: `crates/pex/src/reader.rs:150-161` (`value` → `BadValueType`), `:139-148` (`string_index` → `BadStringIndex`), `:463-497` (`read_instructions` → `BadOpcode`, `BadVarArgCount`)
- No test constructs `.pex` bytes triggering `BadValueType`/`BadOpcode`/`BadVarArgCount`/`BadStringIndex`. Manual review confirms all four implementations are currently correct — pure coverage gap.
- Suggested fix: add four hand-built-`.pex` regression tests, mirroring `hostile_vararg_count_errors_instead_of_ooming`/`rejects_bad_magic`.
