# #3713 — NIF-2026-08-30-D5-01: four constraint types have real CInfo parsers but stay on is_havok_constraint_stub, so their drift is suppressed and nothing asserts it

**Severity**: MEDIUM · **Dimension**: Collision/Shader Parsing
**Location**: `crates/nif/src/lib.rs::is_havok_constraint_stub`, `crates/nif/src/corpus.rs`

## Premise correction (verified before implementing)

The issue's suggested fix names **four** decoded types
(`bhkRagdollConstraint`, `bhkLimitedHingeConstraint`, `bhkHingeConstraint`
#3330, `bhkMalleableConstraint`) and instructs keeping
`bhkPrismaticConstraint` on the stub list as one of "the five genuinely
name-only types". That premise is stale: **#3792**, filed and closed
*after* this issue but evidently fixed earlier in real time, gave
`bhkPrismaticConstraint` typed CInfo decoders too
(`PrismaticCInfo::parse_fo3` / `parse_oblivion`, both call sites in
`constraints.rs` explicitly commented `// #3792`). Confirmed by reading
the current source before touching anything — per this session's
standing practice of verifying an audit-finding premise against current
code before implementing. The correct current split is **five** decoded
types, **four** remaining name-only stubs
(`bhkBallAndSocketConstraint`, `bhkStiffSpringConstraint`,
`bhkGenericConstraint`, `bhkBallSocketConstraintChain`).

## Fix

Narrowed `is_havok_constraint_stub` to the four genuinely name-only
types, so all five decoded types' motor-tail residual now routes through
the real `drift_histogram` instead of the suppressed
`stubbed_drift_histogram` — closing exactly the blind spot that hid the
historic `bhkHingeConstraint` +128 under-read (a whole missing parser)
until #3330 found it by hand.

Added `corpus::is_known_constraint_motor_tail_drift(type_name, drift)` —
a pure predicate for the by-design "motor left for `block_size` recovery"
residual, characterised against nif.xml's `bhkConstraintMotorCInfo`
(1-byte `hkMotorType` discriminator + conditional payload): `1`
(`MOTOR_NONE`), `18` (`bhkSpringDamperConstraintMotor`, 17 B), `19`
(`bhkLimitedForceConstraintMotor`, 18 B, `MOTOR_VELOCITY`), `26`
(`bhkPositionConstraintMotor`, 25 B). `bhkMalleableConstraint`'s own
residual additionally stacks its trailing `Strength: f32` (4 B) on top of
its *wrapped* inner type's own motor-tail drift — `{4}` alone for a
non-motor inner (BallAndSocket/StiffSpring) or `{5, 22, 23, 30}` (each
base value + 4) for a motor-bearing inner. This composition rule was
**corrected once already during this fix** — see the real-data note
below.

Placed the predicate in `corpus.rs` (not `lib.rs`) since it needs to be
callable from an integration test crate — `corpus` is this crate's
existing home for exactly that ("shared conventions... both the
`nif_stats` example and the `tests/common` baseline harness" need).

## Real-data verification (not guessed — measured)

Built and ran `nif_stats --drift-histogram` (`--release`) against all
four affected games' base Meshes archives on this machine:

| game | events | values observed |
|---|---|---|
| Oblivion | 0 | (no `block_sizes` table pre-20.2.0.7 — drift detection doesn't apply at all, consistent with the issue's own table listing zero Oblivion rows) |
| Fallout 3 | 860 | `+26`, `+1` (Ragdoll/LimitedHinge), `+5`/`+4` (Malleable), `+1` (Prismatic) |
| Fallout NV | 1,115 | `+26`, `+1`, `+5` |
| Skyrim SE | 1,575 (matches the issue's own cited count exactly) | `+1` only |

**My first draft of the composition rule was wrong** — it modeled
Malleable's residual as the bare 4-value set *or* a flat `+4`, missing
that FO3's real +5 (59 occurrences) is `1 + 4` (a `MOTOR_NONE` inner
stacked with the Strength trailer), not a value the first draft's
`MOTOR_TAIL_DRIFTS.contains(&drift) || (Malleable && drift == 4)` logic
accepted. Caught immediately by running against real FO3 data before
writing the corpus test, corrected to the additive model above, and
reran against all four games with zero anomalies (4,069 total events,
all within the known set).

## SIBLING (issue's own checklist item)

No `bhk*` dispatch-arm/`resolve_shape` name changed — only which bucket a
type's drift routes into. `resolve_shape` and the shape-dispatch table are
untouched.

## TESTS (issue's own checklist item)

Unit tests in `corpus.rs` (no corpus needed): the four bare motor-tail
values accepted for every decoded type; Malleable's additive
`{4, 5, 22, 23, 30}` set accepted while the *bare* four values are
rejected for Malleable specifically; the historic +128 rejected for any
type.

Real-corpus regression test `tests/constraint_drift_corpus.rs`
(`#[ignore]`d, matching this crate's established convention for
real-game-data tests, e.g. `oblivion_stream_drift_corpus.rs`): re-parses
Oblivion/FO3/FNV/SkyrimSE's mesh archives via `common::
open_all_mesh_archives`, asserts every `drift_histogram` entry for a
decoded constraint type is `is_known_constraint_motor_tail_drift`. Run
against real data: **4,069 events, 0 anomalies**.

**Reintroduce-and-revert verification, two separate probes**:
1. Unit level — confirmed `known_motor_tail_drifts_are_accepted_for_any_decoded_type`
   and the Malleable test fail when the composition rule regresses (checked
   during development against the corrected vs. first-draft logic).
2. Corpus level — dropped `1` from `MOTOR_TAIL_DRIFTS` (replaced with a
   sentinel `999`), reran the real-data corpus test: failed with 3,127+
   listed violations across all four games (the dominant `+1` residual).
   Restored the fix and reran — passes again with the same 4,069-event,
   zero-anomaly result.

## Verification

- `cargo check -p byroredux-nif --tests`: clean, zero warnings.
- `cargo test -p byroredux-nif --lib corpus::`: 7 tests passing, 0
  failing (+3 new).
- `cargo test -q -p byroredux-nif`: 1227 tests passing (+3), 0 failing.
- `cargo test --release -p byroredux-nif --test constraint_drift_corpus -- --ignored --nocapture`:
  1 passing against real 4-game corpus (4,069 events, 0 anomalies).
- `cargo test -q --no-fail-fast` (full workspace): **7132 passing, 0
  failing**.
