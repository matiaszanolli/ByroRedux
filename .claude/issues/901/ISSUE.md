# #901 — FNV-D5-NEW-02: ROADMAP FNV ESM record count (62 219) stale — actual is 73 054

**Severity**: LOW (documentation drift)
**Source audit**: `docs/audits/AUDIT_FNV_2026-05-08.md` § Dimension 5
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/901
**Status**: NEW · CONFIRMED at HEAD `318fcaf`

## Locations

- `ROADMAP.md:70` — Status prose: "FNV; 62 219 records on the latest sweep."
- `ROADMAP.md:561` — Repro commands table row: "Full ESM record counts (FNV 62 219 / FO3 31 101)"

## Summary

`parse_rate_fnv_esm` reports `total = 73 054` plus a 5 625-record long tail bucket. ROADMAP pins `62 219` in two locations.

Repro: `cargo test -p byroredux-plugin --release --test parse_real_esm -- --ignored fnv`

The ~10 800-record jump is fully explained by session 29's #808/#809/#810 long-tail dispatch closeout (37+ new record types). FO3's `31 101` claim has the same shape — predates the same closeout — but this audit was FNV-only.

## Fix path

1. ROADMAP.md:70 → `73 054 records`
2. ROADMAP.md:561 → `73 054`
3. Re-run `parse_real_esm` for FO3 to surface matching FO3 drift (separate follow-up)

## Related

- #808 / #809 / #810 — session 29 ESM dispatch closeouts
