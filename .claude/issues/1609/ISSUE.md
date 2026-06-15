# #1609 — NIF-D5-04: FO3+ malleable inner-dispatch lacks size-skip

_Filed from `docs/audits/AUDIT_NIF_2026-06-14.md` via /audit-publish. Immutable snapshot as-filed; GitHub is authoritative for current state._

**Severity**: LOW (`block_size`-absorbed on all FO3+ files, which always carry the table; an asymmetry, not a mis-parse) · **Dimension**: Collision/Shader Parsing · **Status**: NEW (acceptable-but-asymmetric; documents a real behaviour)
**Source**: AUDIT_NIF_2026-06-14 (NIF-D5-04)
**Game Affected**: FO3 / FNV / Skyrim / FO4+ (malleable-wrapped constraints whose inner type is not Ragdoll/LimitedHinge).

**Location**: [collision/constraints.rs:241-251](crates/nif/src/blocks/collision/constraints.rs#L241-L251) (`parse_fo3_malleable_inner` — `_ => BhkConstraintData::Other` reads nothing further).

## Description
When a `bhkMalleableConstraint` wraps an inner type other than Ragdoll (7) or LimitedHinge (2), the FO3+ path returns `Other` without consuming the inner CInfo bytes, relying entirely on the outer `block_size` seek. The Oblivion path (which has no `block_sizes` anchor) *does* size-skip the fixed payload of undecoded types. The two paths are asymmetric: the FO3+ path would mis-parse if it ever ran on a sizeless file, and it offers no defence-in-depth.

## Evidence
`constraints.rs:248` (`_ => BhkConstraintData::Other`) vs the Oblivion `payload_size` size-skip table (`constraints.rs:~299-321`).

## Impact
None today (FO3+ always carries `block_sizes`). Latent if the malleable path is ever exercised on a sizeless format.

## Related
NIF-D5-03 (same new code); #1539 (import-side `extract_ragdoll` silent-drop of the same `Other` constraints — distinct layer).

## Suggested Fix
Mirror the Oblivion size-skip: for the `_` arm, `stream.skip()` the fixed FO3+ inner-CInfo payload size by wrapped type so consumption is self-consistent independent of `block_size`.

## Completeness Checks
- [ ] **SIBLING**: Ensure the FO3+ payload-size table matches the Oblivion `payload_size` table for every wrapped constraint type
- [ ] **TESTS**: A regression test pins inner-CInfo consumption on a malleable-wrapped non-Ragdoll/non-LimitedHinge instance
