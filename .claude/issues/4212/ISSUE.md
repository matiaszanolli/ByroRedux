# TD6-001: Four Havok constraint block types are base-only stubs (no CInfo decode)

Labels: medium,tech-debt,physics,nif,bug

**Description**: `bhkBallAndSocketConstraint`, `bhkStiffSpringConstraint`, `bhkGenericConstraint`, and `bhkBallSocketConstraintChain` fall through to the base `NiTimeController`-shaped stub — CInfo fields never decoded, only skipped via block-size recovery. This is a residual gap distinct from closed #117/#979, which fixed the *cascading-failure* dispatch bug for these types (the engine no longer crashes/misparses on them) but did not add their constraint-parameter decode. Kept out of the main `drift_histogram` via a parallel counter specifically so it doesn't drown real parser-drift signal. Genuinely still open; affects Oblivion->Skyrim-era ragdolls/physics props where PHYSAL is otherwise "converged."

**Evidence**:
`crates/nif/src/lib.rs:189` (`is_havok_constraint_stub`), consumed at `:499`; dispatch in `crates/nif/src/blocks/mod.rs`; telemetry field `crates/nif/src/scene.rs:108` (`stubbed_drift_histogram`).

**Impact**: Constraint parameters (limits, motor settings, spring stiffness, chain link topology) are silently dropped for any NIF using these four Havok constraint types — the ragdoll/physics prop behaves as if unconstrained past whatever the base stub captures.

**Related**: Closed #117 (NIF-515, cascading-failure fix), closed #979 (bhkBallSocketConstraintChain dispatch arm).

**Suggested Fix**: Implement the four CInfo decoders (mirroring existing `bhkHingeConstraint`/`bhkRagdollConstraint` parsers), or explicitly re-confirm in ROADMAP.md/PHYSAL docs that this is a known permanent gap with an issue link. Partially coupled to PHYSAL's broader FO4+ `BhkSystemBinary` blocker for the FO4+ side, but affects Oblivion->Skyrim content independently of that blocker.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_TECH_DEBT_2026-09-11.md`.*
