# LC-D2-NEW-02: parse_nif does not invoke validate_refs

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/892
**Source audit**: docs/audits/AUDIT_LEGACY_COMPAT_2026-05-07.md
**Severity**: LOW (defensive-coding / observability gap)
**Dimension**: NIF Format Readiness — link integrity

## Location
- crates/nif/src/lib.rs:135-165 (parse_nif / parse_nif_with_options)
- crates/nif/src/scene.rs:106-197 (NifScene::validate_refs — opt-in, never invoked)

## Root Cause
`validate_refs` exists, has full coverage of every BlockRef-bearing field via `HasObjectNET` / `HasAVObject` / `HasShaderRefs` plus an explicit NiNode children/effects downcast — but no caller invokes it outside the validate_refs_tests module. nif_stats does not run it. The walker and cell loader rely on `scene.get(idx)` returning None for out-of-range indices, which is structurally fine but silently drops the dangling-ref count.

## Why it matters
`recovered_blocks > 0` does NOT imply dangling refs. A parser regression that under-consumes and shifts the next block by N bytes would leave a downstream BlockRef pointing into the middle of another block — visible only as a render artifact, not a parser-level signal.

## Suggested Fix (three options)

1. **Minimum-surface**: opt-in `ParseOptions::validate_links: bool` + `NifScene.link_errors: usize`.
2. **Default-on**: run validation unconditionally in parse_nif_with_options; route into the parse-rate gate.
3. **Integration-only**: run only in tests/parse_real_nifs.rs with per-game histogram.

Option 1 = lowest blast radius; Option 3 = cheapest in hot-path overhead.

## Verification

```bash
grep -rn validate_refs crates/ byroredux/ tools/
# expected: only validate_refs_tests hits before fix
```

## Related
- #839: per-block stream realignments invisible to nif_stats parse-rate gate (same observability theme)
- #568: recovered_blocks field that link_errors would extend
