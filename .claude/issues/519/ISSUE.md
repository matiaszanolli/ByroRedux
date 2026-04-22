# FNV-ESM-3: AVIF not dispatched

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/519
- **Severity**: MEDIUM
- **Dimension**: ESM record parser
- **Audit**: `docs/audits/AUDIT_FNV_2026-04-21.md`
- **Status**: NEW (created 2026-04-21)

## Location

`crates/plugin/src/esm/records/mod.rs:243-406` — no `b"AVIF"` arm

## Summary

AVIF FourCC defined in `record.rs:217` and referenced by NPC skill_bonuses + BOOK skill forms, but `parse_esm` has no dispatch arm. All AVIF records hit `skip_group`. Blocks perk pipeline, VATS cost read, AVIF-keyed condition predicates.

Fix: add `parse_avif` stub in `records/misc.rs` (EDID / FULL / av_type / default_value).

Fix with: `/fix-issue 519`
