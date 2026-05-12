# Issue #979

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/979
**Title**: NIF-D5-NEW-03: bhkBallSocketConstraintChain missing dispatch arm — Oblivion ragdoll spine cascades
**Labels**: bug, nif-parser, medium
**Audit source**: docs/audits/AUDIT_NIF_2026-05-12.md

---

**Source**: `docs/audits/AUDIT_NIF_2026-05-12.md` (Dim 5)
**Severity**: MEDIUM
**Dimension**: Coverage
**Game Affected**: Oblivion (cascade), FO3+ (silent drop via block_size recovery)
**Location**: missing from `crates/nif/src/blocks/mod.rs:949-969` (Havok constraint stub group)

## Description

`nif.xml` defines `bhkBallSocketConstraintChain` with `versions="#BETHESDA#"` (Oblivion onward, BSHavok module). It's the Havok class behind ragdoll spine joints (neck → head) and other multi-segment joint chains — referenced by **every** FO3+/Oblivion humanoid ragdoll.

The existing constraint dispatch group at `mod.rs:949-969` covers 7 sibling `bhk*Constraint` types but not the chain variant:

```rust
"bhkBallAndSocketConstraint" | "bhkHingeConstraint" | "bhkLimitedHingeConstraint"
    | "bhkRagdollConstraint" | "bhkPrismaticConstraint" | "bhkStiffSpringConstraint"
    | "bhkMalleableConstraint" => { /* stub-skip body */ }
```

`bhkBallSocketConstraintChain` falls through to the terminal `_ =>` fallback.

## Impact

- **Oblivion**: no `block_sizes` table — the missing arm cascades, truncating the rest of the NIF. NPCs render with detached heads / broken physics falls.
- **FO3+**: `block_size` recovery covers the parse cleanly but the chain constraint silently drops, producing the same detached-head ragdoll effect at runtime when the NPC dies.

## Suggested Fix

Add `bhkBallSocketConstraintChain` to the constraint alias group at `mod.rs:949-969` with the existing stub-skip body. The `is_havok_constraint_stub` helper at `crates/nif/src/lib.rs:127-139` already lists the 7 stubbed types — add the 8th here too so the drift detector treats it consistently.

```rust
// blocks/mod.rs:949
"bhkBallAndSocketConstraint" | "bhkHingeConstraint" | "bhkLimitedHingeConstraint"
    | "bhkRagdollConstraint" | "bhkPrismaticConstraint" | "bhkStiffSpringConstraint"
    | "bhkMalleableConstraint" | "bhkBallSocketConstraintChain" => {
    /* same stub-skip body */
}

// lib.rs:127-139 (is_havok_constraint_stub)
"bhkBallAndSocketConstraint" | "bhkHingeConstraint" | "bhkLimitedHingeConstraint"
    | "bhkRagdollConstraint" | "bhkPrismaticConstraint" | "bhkStiffSpringConstraint"
    | "bhkMalleableConstraint" | "bhkBallSocketConstraintChain" => true,
```

## Completeness Checks

- [ ] **TESTS**: Fixture test with a real Oblivion humanoid ragdoll NIF (any meshes\characters\_male\skeleton.nif equivalent); verify the parse no longer cascades
- [ ] **SIBLING_CHECK**: Are there OTHER `versions="#BETHESDA#"` Havok types in nif.xml not in the dispatch group? Specifically scan for `module="BSHavok"` entries
- [ ] **DRIFT_HISTOGRAM**: After fix, `bhkBallSocketConstraintChain` row should appear in the stubbed-drift bucket (per #939 + NIF-D3-NEW-06 telemetry recommendation), confirming the stub fires
- [ ] **OBLIVION_REGRESSION**: Parse-rate sweep across Oblivion `Oblivion - Meshes.bsa` before/after; the missing-arm cascade may be a contributor to the 95.21% baseline
- [ ] **HAVOK_TRANSFORM**: After parse, the import path doesn't currently consume constraints — but verify the existing `extract_collision` doesn't choke on the new stub block type (Box<NiUnknown> vs new type)

## Audit reference

`docs/audits/AUDIT_NIF_2026-05-12.md` § Findings → MEDIUM → NIF-D5-NEW-03.

