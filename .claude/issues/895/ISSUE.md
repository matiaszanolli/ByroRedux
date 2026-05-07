# LC-D6-NEW-03: StringPool::resolve returns lowercased canonical form — case loss undocumented

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/895
**Source audit**: docs/audits/AUDIT_LEGACY_COMPAT_2026-05-07_DIM6.md
**Severity**: LOW (intentional design choice; impact is cosmetic + doc gap)
**Dimension**: String Interning — display fidelity

## Location
- crates/core/src/string/mod.rs:31-40 (intern lowercases, resolve returns lowercased)
- docs/legacy/api-deep-dive.md § "NiFixedString — String Interning" (claims alignment without flagging case-folding divergence)
- crates/debug-server/src/evaluator.rs:654 (consumer affected by lowercased EDIDs)

## Root Cause
`intern` lowercases via `to_ascii_lowercase`. `resolve` returns the canonical lowercased form. No path back to original case once interned.

Gamebryo's `efd::FixedString` preserves case; case-insensitive comparison is opt-in via `EqualsNoCase` / `ContainsNoCase`. Redux baked case-insensitivity into the pool itself — fine for path/animation matching, but lossy for display.

## Concrete consequences
1. EDIDs in console output: `DocMitchell` → `docmitchell`
2. Animation channel names in script output: `Bip01 Spine` → `bip01 spine`
3. Name component for UI/book text loses authoring case (workaround: production already uses Arc<str> on ImportedNode/ImportedMesh for case-preserving paths)

## Suggested Fix (pick one)

1. **Documentation-only (recommended)** (~5 LOC): update `docs/legacy/api-deep-dive.md` to surface the divergence and point at the existing `Arc<str>` lane for case-preserving use; annotate `StringPool::resolve` doc more loudly.
2. **Keep both** (~30 LOC): store `(lowercased_lookup, original_case_string)`; resolve returns first-seen original. Costs ~2× pool memory.

Option 1 today — no consumer needs case-preserving resolve.

## Related
- #866 (OPEN): `AnimationClipRegistry::get_or_insert_by_path` doesn't lowercase — same case-handling drift theme; pair the fix
