# FNV-ESM-2: Two-pass walk over full ESM on every parse_esm call

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/527
- **Severity**: LOW
- **Dimension**: ESM / performance
- **Audit**: `docs/audits/AUDIT_FNV_2026-04-21.md`
- **Status**: NEW (created 2026-04-21)

## Location

`crates/plugin/src/esm/records/mod.rs:186-208`

## Summary

`parse_esm` walks the full TES4 slice twice (cells + statics first pass, typed records second). FNV cold parse is 1.21s; FO4/Skyrim/Starfield will hurt more. Fuse into single dispatcher with Cell/Records target enum.

Fix with: `/fix-issue 527`
