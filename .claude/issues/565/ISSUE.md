# #565 — SK-D5-04: bhkRigidBody recovery-path warning spam

**Severity:** MEDIUM
**Labels:** bug, medium, nif-parser
**Source:** AUDIT_SKYRIM_2026-04-22.md (companion to #546)
**GitHub:** https://github.com/matiaszanolli/ByroRedux/issues/565

## Location
- `crates/nif/src/lib.rs:302-312`

## One-line
Single sweetroll demo already logs `Block N 'bhkRigidBody' (size 250, consumed 215): NIF claims 3503082814 elements…`. Full cell load will burst ~14,408 lines.

## Fix sketch
Preferred: fix #546 (SK-D5-01). Fallback: downgrade bhkRigidBody recovery to `log::debug!` with once-per-archive aggregate summary at warn level.

## Depends on
- #546 (parent — real parser fix)

## Next
`/fix-issue 565` (or close if #546 lands first)
