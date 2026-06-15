# #1604 — NIF-D5-01: bhkBallSocketConstraintChain reads the wrong base struct

_Filed from `docs/audits/AUDIT_NIF_2026-06-14.md` via /audit-publish. Immutable snapshot as-filed; GitHub is authoritative for current state._

**Severity**: MEDIUM · **Dimension**: Collision/Shader Parsing · **Status**: NEW (pre-existing from #979; surfaced by this byte-audit)
**Source**: AUDIT_NIF_2026-06-14 (NIF-D5-01)
**Game Affected**: Skyrim SE / LE (bsver 83–100) — the only vanilla home of this type (~6 instances); also any FO4+ Bethesda chain content.

**Location**: [blocks/mod.rs:1153-1154](crates/nif/src/blocks/mod.rs#L1153-L1154) (dispatches `bhkBallSocketConstraintChain` → `BhkConstraint::parse`), [collision/constraints.rs:225-231](crates/nif/src/blocks/collision/constraints.rs#L225-L231) (`parse_base`), [lib.rs:179](crates/nif/src/lib.rs#L179).

## Description
nif.xml declares `bhkBallSocketConstraintChain` with `inherit="bhkSerializable"`, **not** `bhkConstraint`. `BhkConstraint::parse` unconditionally calls `parse_base`, which reads `num_entities:u32 + entity_a:ref + entity_b:ref + priority:u32` (16 B) — fields that do not exist at the head of a `bhkSerializable`-derived chain. The first 16 bytes of the chain's real payload are reinterpreted as those four fields.

## Evidence
A traced Skyrim instance declared `block_size=260`; the parser consumed 16 B and the bytes that became `entity_a`/`entity_b`/`priority` are actually the chain's pivot-float / count data. nif.xml confirms `inherit="bhkSerializable"`. On Skyrim the 244-byte remainder is seeked past via `block_size`, so the scene survives; the stored entity refs are garbage but no current consumer reads them.

## Impact
The chain constraint is dropped (never reaches a future ragdoll consumer) and stores garbage refs. Today: invisible (unused fields, `block_size`-recovered). Tomorrow: if a consumer trusts `entity_a/entity_b`, or if this type appears in any sizeless-format file, it cascades.

## Related
#979; PHYSAL (this type is on the ragdoll-constraint path the M41.x work targets).

## Suggested Fix
Either give `bhkBallSocketConstraintChain` its own parser that reads the `bhkSerializable` chain layout (`num_pivots`, pivots, constraint params, then the `entity` array per nif.xml), or — until a consumer needs it — dispatch it to a name-only opaque stub that reads zero base bytes and relies on `block_size` recovery, rather than fabricating four wrong fields.

## Completeness Checks
- [ ] **SIBLING**: Same wrong-base-struct pattern checked in the other `bhkSerializable`-derived dispatch arms in `blocks/mod.rs`
- [ ] **TESTS**: A regression test pins the chain's base-byte consumption against a traced Skyrim instance's `block_size`
