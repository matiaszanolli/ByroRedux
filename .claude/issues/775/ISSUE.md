# Issue #775: FO3-6-02 — CLAUDE.md Usage block lacks FO3 CLI example

**Severity**: LOW · **Domain**: documentation · **Type**: enhancement
**Source audit**: docs/audits/AUDIT_FO3_2026-05-01.md

## Summary

CLAUDE.md:210-217 lists FNV interior/exterior + Skyrim DLC examples but no FO3 example. ROADMAP claims FO3 Tier-1 status; the canonical Megaton bench command is undocumented in the developer-facing CLAUDE.md.

## Fix

Add one Usage line: `cargo run -- --esm Fallout3.esm --cell Megaton01 --bsa "Fallout - Meshes.bsa" --textures-bsa "Fallout - Textures.bsa"` — FO3 interior cell.
