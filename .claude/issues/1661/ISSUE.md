**Severity**: LOW · **Dimension**: BSA v105 (LZ4)
**Location**: `byroredux/src/asset_provider.rs:374` (`open_with_numeric_siblings`; `if stem.chars().last().is_some_and(|c| c.is_ascii_digit()) { return; }` at `:398`)
**Status**: NEW (confirmed still present 2026-06-18; documented intentional behavior, never filed)

## Description
`open_with_numeric_siblings` auto-loads `<stem>2.bsa`..`<stem>9.bsa` only when the named archive's stem does not already end in a digit. Skyrim's base textures ship as `Skyrim - Textures0.bsa`..`Textures8.bsa` — the stem ends in `0`, so passing `--textures-bsa "Skyrim - Textures0.bsa"` loads only archive 0; Textures1–8 are silently not loaded. Documented intentional behavior (the FNV `Foo.bsa`/`Foo2.bsa` unnumbered-base case is the target).

## Evidence
- `byroredux/src/asset_provider.rs:398`: `if stem.chars().last().is_some_and(|c| c.is_ascii_digit()) { return; }` — digit-suffixed stems skip the sibling sweep entirely.

## Impact
If a user passes only `Textures0.bsa`, most texture entries resolve to the missing-texture checkerboard → "chrome/posterized" surfaces. Operator UX, not data corruption.

## Suggested Fix
Optionally sweep the remaining `<stem>0..9.bsa` siblings when a digit-suffixed base is passed (dedup on resolved path). Or keep the docs and treat as WONTFIX.

## Completeness Checks
- [ ] **SIBLING**: Both the meshes and textures BSA auto-load call sites get the same digit-suffixed sibling behavior
- [ ] **TESTS**: A regression test pins that passing `Textures0.bsa` resolves siblings `Textures1..8` (or documents the WONTFIX decision)
