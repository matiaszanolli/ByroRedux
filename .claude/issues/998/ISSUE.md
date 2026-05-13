# #998 — SPT-D3-01: No in-tree byte-stable SPT fixture for CI

- **Severity**: LOW
- **Domain**: enhancement / legacy-compat
- **Audit**: `docs/audits/AUDIT_SPEEDTREE_2026-05-13.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/998

## TL;DR
Corpus harness (`parse_real_spt.rs`) is env-var-gated; the synthetic fixture in `parser.rs` is hand-built. No SHA-pinned vanilla-shape sample lives in-tree, so CI without game data has zero corpus coverage.

## Fix
Preferred: deterministic synthetic SPT generator under a `#[test]` helper that emits real-byte-shape fixtures covering every dispatch arm, SHA-pinned. Matches clean-room policy (no vanilla redistribution).
