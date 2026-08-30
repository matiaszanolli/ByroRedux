# #3713: NIF-2026-08-30-D5-01: four constraint types have real CInfo parsers but stay on is_havok_constraint_stub, so their drift is suppressed and nothing asserts it

**Labels**: bug, nif-parser, medium, nif, physics, test-gap
**Filed**: 2026-08-30 (audit-publish)

---

**Report**: `docs/audits/AUDIT_NIF_2026-08-30.md` · **Severity**: MEDIUM · **Dimension**: Collision/Shader Parsing
**Game affected**: Oblivion, Fallout 3, Fallout NV, Skyrim SE (the games whose corpora contain `bhk*Constraint` blocks; FO4/FO76/Starfield ship none)

## Location
- `crates/nif/src/lib.rs` — `is_havok_constraint_stub` (currently `:185-199`), consumed at `:471-484`
- `crates/nif/src/blocks/collision/constraints.rs` — the real CInfo parsers it shadows

## Description
`is_havok_constraint_stub` lists nine constraint type names and routes any drift on them into `stubbed_drift_histogram` instead of `drift_histogram`, explicitly so that "a future audit running `nif_stats --drift-histogram` doesn't see ~45 systematic under-reads per skeleton load and falsely conclude constraints parse cleanly".

**Four of those nine** — `bhkRagdollConstraint`, `bhkLimitedHingeConstraint`, `bhkHingeConstraint` (#3330) and `bhkMalleableConstraint` — are no longer name-only stubs; they have typed CInfo parsers whose residual is a small, known, fully-predictable tail. They are still on the list, so a genuine regression inside `RagdollCInfo::parse_fo3` or `LimitedHingeCInfo::parse_fo3` lands in the same bucket as the intended motor tail, indistinguishable from it.

The bucket's stated purpose — "spot a new stub regression (constraint type drifts from its expected stub size)" — **has no implementation**: no test, gate or baseline reads `stubbed_drift_histogram` or asserts any expected value.

## Evidence
`is_havok_constraint_stub` returns `true` for all nine names including the four with real parsers (verified against current source).

**`bhkHingeConstraint` is the proof this matters.** A sweep of the same corpora taken 2026-08-27 recorded:

```
bhkHingeConstraint  drift=+128  count=6   (FO3)
bhkHingeConstraint  drift=+128  count=4   (FNV)
bhkHingeConstraint  drift=+128  count=8   (Skyrim SE)
```

+128 is exactly the full FO3+ `bhkHingeConstraintCInfo` — 8 × `Vector4` (nif.xml:2457-2464) — i.e. the constraint's entire payload was undecoded on **100% of instances in three games**. That is not a motor tail; it is a whole missing parser, and it sat inside the suppressed bucket where no gate looks until #3330 went hunting by hand. The 2026-08-30 sweep confirms #3330 fixed it, but the routing that concealed it is unchanged for the three remaining real parsers.

Current residual on every constraint type, characterised byte-for-byte against nif.xml — which is what makes it assertable:

| observed drift | composition | nif.xml sizes |
|---|---|---|
| +1 (FNV 1,301 · FO3 931 · SkyrimSE 1,575) | motor type byte, `MOTOR_NONE` | 1 (`hkMotorType` is a `byte`, nif.xml:2370) |
| +18 (FO3 10) | type byte + `bhkSpringDamperConstraintMotor` | 1 + 17 (`size="17"`) |
| +26 (FO3 34 · FNV 1) | type byte + `bhkPositionConstraintMotor` | 1 + 25 (`size="25"`) |
| +5 / +4 (`bhkMalleableConstraint`, FNV 143 · FO3 60) | motor byte + `Strength` f32 / `Strength` alone | 1 + 4 / 4 |
| +32 (`bhkBallAndSocketConstraint`, SkyrimSE 30) | whole undecoded CInfo | `size="32"` |
| +36 (`bhkStiffSpringConstraint`) | whole undecoded CInfo | 2×Vec4 + f32 = 36 |
| +141 (`bhkPrismaticConstraint`, FO3 9 · FNV 3) | whole undecoded CInfo + motor byte | 140 + 1 |

## Impact
Instrumentation, not live corruption — the current residuals are all correct, and FO3+ files are never sizeless so `block_sizes` keeps the outer stream aligned. The exposure is that the one telemetry surface able to notice a constraint-decode regression deliberately discards it for exactly the types that now have something to regress, and the hinge case shows the blind spot can persist across many releases. Constraint CInfo decode is also the sole per-game seam in PHYSAL, so a silent drift here mislabels ragdoll joint frames rather than failing loudly.

## Related
#117 (the original stubs), #3330 (the fix whose absence this blind spot hid), #979.

## Suggested Fix
Narrow `is_havok_constraint_stub` to the five genuinely name-only types (`bhkBallAndSocketConstraint`, `bhkPrismaticConstraint`, `bhkStiffSpringConstraint`, `bhkGenericConstraint`, `bhkBallSocketConstraintChain`) so the four decoded types report through the real `drift_histogram`. Then replace the suppression with an assertion: the residual for a decoded constraint is now known to be exactly 1, 18, 26, or 4/5, so a per-game gate asserting membership in that set turns the invisible +128 class into a red build. The values are already measured per game in the table above.

## Completeness Checks
- [ ] **SIBLING**: adding/removing a `bhk*` name here needs its `resolve_shape` / dispatch-arm counterpart checked (see the shape-dispatch parity rule)
- [ ] **TESTS**: A regression test pins this specific fix — assert the measured residual set per game
