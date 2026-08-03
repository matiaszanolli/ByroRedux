# SF-D5-2026-08-03-01: Stale Skyrim+16-byte-tail framing survives in a test assertion message

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2364
**Labels**: bug,nif-parser,low,legacy-compat

---

**Severity**: LOW
**Dimension**: 5 — ESM + Cell Bring-up Regression Surface (Starfield audit, 2026-08-03)
**Location**: `crates/plugin/src/esm/cell/walkers.rs:172-174` (assertion message in `starfield_xcll_sizes_pinned`), doc comment at `:39`
**Status**: NEW, CONFIRMED against current code

## Description

#1293 corrected the module doc comment and the test's own docstring to say Starfield's 108-byte XCLL "shares only bytes 0-39 with Skyrim, then diverges into a distinct volumetric height-fog model" — but the `assert_eq!` failure-message string in `starfield_xcll_sizes_pinned` (added by the earlier #1291 commit, untouched by #1293) still reads "Skyrim+ 92-byte body + 16-byte SF tail," exactly the disproven framing, three lines below the corrected docstring.

## Evidence

Confirmed by direct read: `walkers.rs:39` doc comment says "shares only bytes 0-39"; `walkers.rs:172-174` assertion message still says `"Starfield's vanilla XCLL is 108 bytes (Skyrim+ 92-byte body + 16-byte SF tail). See #1291."`

## Impact

No functional/parsing impact — decode logic and canonical-size table are correct and byte-verified. Impact is confined to future maintainers if this pinned assertion ever fires and they read the stale message.

## Suggested Fix

Update the assertion message to match the corrected docstring's framing (bytes 0-39 shared, then diverges into a distinct height-fog model).

## Completeness Checks
- [ ] **TESTS**: N/A — this IS the test; the fix is a string-literal correction with no behavior change
