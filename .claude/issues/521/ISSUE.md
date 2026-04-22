# FNV-ESM-10: ACTI / TERM parsed only as MODL statics

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/521
- **Severity**: MEDIUM
- **Dimension**: ESM record parser
- **Audit**: `docs/audits/AUDIT_FNV_2026-04-21.md`
- **Status**: NEW (created 2026-04-21)

## Location

`crates/plugin/src/esm/records/mod.rs` — no `b"ACTI"` / `b"TERM"` arms

## Summary

Activators and Terminals register only as MODL statics in `cells.statics`. SCRI cross-refs dangle; terminal menu trees (MNAM/BSIZ/ANAM) are lost. Vault terminals, arena doors, NukaCola vending all affected once interaction systems land.

Fix: add `parse_acti` + `parse_term` stubs extracting EDID/FULL/MODL/SCRI/DEST + terminal-specific menu sub-records.

Fix with: `/fix-issue 521`
