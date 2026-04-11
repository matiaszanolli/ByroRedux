# Issue #224 — D2-03

**Title**: Oblivion NIFs have no block sizes — no skip recovery for unknown types
**Severity**: LOW (resilience/enhancement)
**Audit**: docs/audits/AUDIT_LEGACY_COMPAT_2026-04-10.md

## Fix
Added `ParseOptions.oblivion_skip_sizes: HashMap<String, u32>` — callers can register a size hint per type, and the main parse loop consults it on error before truncating. Also exposed `NifScene.dropped_block_count: usize` so observability layers can quantify truncation.

Regression tests in [crates/nif/src/lib.rs](../../../crates/nif/src/lib.rs) cover:
- Recovery with hint (3 unknown blocks, 0 dropped, not truncated)
- Fallback on oversized hint (past EOF → truncation path, never panics)
- Default (no hint) behavior unchanged
