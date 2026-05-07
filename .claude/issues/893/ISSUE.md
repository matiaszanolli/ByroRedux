# LC-D6-NEW-02: FixedString carries no StringPool provenance — silent cross-pool foot-gun

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/893
**Source audit**: docs/audits/AUDIT_LEGACY_COMPAT_2026-05-07_DIM6.md
**Severity**: LOW (latent foot-gun, no current production multi-pool path)
**Dimension**: String Interning — type safety

## Location
- crates/core/src/string/mod.rs:10 (`pub type FixedString = string_interner::DefaultSymbol;`)
- crates/core/src/animation/registry.rs:250-252 (test fixture demonstrating the gap)

## Root Cause
`FixedString` is a transparent alias for `string_interner::DefaultSymbol` (u32). The same u32 can resolve to different strings in different pools — silently incorrect, never type-error. Gamebryo's `efd::FixedString::m_handle` is a `Char*` into the singleton GlobalStringTable buffer; provenance is structurally enforced.

## Why it matters today
No active bug — production has one canonical pool inserted at `byroredux/src/main.rs:294`. But:
- 11 `StringPool::new()` test instances mint symbols that could escape into shared fixtures
- M40 streaming will likely want per-thread parser-local pools merged at commit; the merge step is currently invisible to the type system

## Suggested Fix (pick one)

1. **Lightweight (recommended)**: newtype wrapper around `DefaultSymbol` with `PhantomData<*const StringPool>` — compile-time provenance check, zero runtime cost, wire size unchanged.
2. **Heavy**: track pool ID in each symbol (~8 bytes), assert at resolve. Runtime panic on misuse, costs 4 bytes per FixedString.

Option 1 is recommended.

## Verification
- Add a UI test (compile-fail) that asserts cross-pool symbol use is a type error
- Re-run cargo build, all production paths compile unchanged

## Related
- FormId at crates/core/src/form_id.rs has the same provenance gap (u64 newtype, no marker)
