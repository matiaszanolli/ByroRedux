# #999 — SPT-D1-01: Walker bails on unknown-in-range tag without skip-resume

- **Severity**: LOW
- **Domain**: enhancement / legacy-compat
- **Audit**: `docs/audits/AUDIT_SPEEDTREE_2026-05-13.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/999

## TL;DR
4 Oblivion `.spt` files bail at `tag=104 (0x0068)` (length prefix of a trailing curve text blob without a leading tag header). Oblivion coverage holds at 96.46% — above the 95% gate, but dictionary refinement would push it to 100%.

## Fix
Run `spt_dissect` at the bail offsets, classify the section, add a `dispatch_tag` arm. No parser shape change. Already noted in `crates/spt/docs/format-notes.md` "Next sub-phase".
