# #1605 — NIF-D5-02: M41.x typed constraint decoders still listed as stubs

_Filed from `docs/audits/AUDIT_NIF_2026-06-14.md` via /audit-publish. Immutable snapshot as-filed; GitHub is authoritative for current state._

**Severity**: MEDIUM (no mis-parse today; a regression-detection blind spot on the single highest-priority new parser code in the tree) · **Dimension**: Collision/Shader Parsing (regression surface) · **Status**: NEW
**Source**: AUDIT_NIF_2026-06-14 (NIF-D5-02)
**Game Affected**: FO3 / FNV / Skyrim / FO4+ (every game with Havok ragdolls).

**Location**: [lib.rs:167-181](crates/nif/src/lib.rs#L167-L181) (`is_havok_constraint_stub`, still lists `bhkLimitedHingeConstraint`, `bhkRagdollConstraint`, `bhkMalleableConstraint`).

## Description
`is_havok_constraint_stub` suppresses the consumed-vs-`block_size` drift `warn!` for the listed types (so actor-spawn logs aren't drowned, #462). Its own doc comment states: *"When a full CInfo parser lands for any of these types, remove it from here so the drift detector goes back to catching real mistakes."* A full CInfo parser landed today (2026-06-14) for `bhkRagdollConstraint` / `bhkLimitedHingeConstraint` / `bhkMalleableConstraint`, yet all three remain in the list. Their drift is therefore still suppressed from `nif_stats --drift-histogram` and the reconciliation `warn!`, so a future **over-read** regression in this byte-exact-but-fragile new decode would be invisible to the standard NIF regression signal.

## Evidence
`lib.rs:172,174,176` list the three now-typed names; [collision/constraints.rs](crates/nif/src/blocks/collision/constraints.rs) is the new typed parser (`RagdollCInfo` / `LimitedHingeCInfo` / `MalleableConstraintCInfo`) for exactly those names.

## Impact
Loss of regression coverage on the most safety-critical new parser. The decoders currently *under*-read by design (motor left for recovery), so they can't simply be removed from the list without re-introducing the #462 warn-spam — but the over-read direction is now unguarded.

## Related
#462, #979, the M41.x PHYSAL commits (`ca631e09`, `0a0bc3ce`).

## Suggested Fix
Replace the blanket suppression for the now-typed types with a *signed* tolerance: suppress the expected small **under**-read (the unread motor/strength tail) but still `warn!` on any **over**-read or larger-than-expected under-read. Alternatively, add a dedicated `block_size`-equality assertion on the typed decoders in the existing constraint unit-test suite so an over-read regression fails CI.

## Completeness Checks
- [ ] **SIBLING**: Apply the same signed-tolerance treatment to every now-typed name removed from the stub list
- [ ] **TESTS**: A `block_size`-equality (or signed-tolerance) assertion pins the typed decoders so an over-read regression fails CI
