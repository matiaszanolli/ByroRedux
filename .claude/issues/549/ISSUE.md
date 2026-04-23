# NIF-04: bhkRigidBody parse fails on 6 Oblivion blocks

**Severity**: HIGH
**Dimension**: Block Parsing
**Game Affected**: Oblivion
**Audit**: docs/audits/AUDIT_NIF_2026-04-22.md § NIF-04

## Summary

6 `bhkRigidBody` blocks in the Oblivion mesh BSA fall into NiUnknown. Small count but shares the Skyrim symptom from NIF-01 (#546) — the parser works for most of the archive but fails on a few specific NIFs. Likely a variable-length cInfo tail that one Oblivion authoring tool emits differently.

## Evidence

`/tmp/audit/nif/obl_unk.out`: `bhkRigidBody` 6.

## Location

`crates/nif/src/blocks/collision.rs` — `BhkRigidBody::parse`.

## Suggested fix

Capture the 6 failing NIFs via `crates/nif/examples/trace_block.rs` and diff against a clean Oblivion rigid body. Likely a 4-byte padding or `body_flags` gate that fires early. Coordinate with NIF-01 — both symptoms may share a common cause.

## Completeness Checks
- [ ] **SIBLING**: NIF-01 (#546) — shared parser, co-fix if root cause is the same gate
- [ ] **TESTS**: Add an Oblivion-bsver-34 rigid-body fixture

Fix with: /fix-issue <number>
