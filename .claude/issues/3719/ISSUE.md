# #3719 — NIF-2026-08-30-D2-02: three NifVersion constants have no call site, including a `V20_2_0_7_SSE` alias whose only reference is a test asserting it equals `V20_2_0_7`

**Severity**: LOW · **Location**: `crates/nif/src/version.rs`
**Source**: `docs/audits/AUDIT_NIF_2026-08-30.md` (NIF-2026-08-30-D2-02)

`V10_1_0_112` (:119), `V20_2_0_7_SSE` (:160), and `V30_1_0_1` (:170) have zero
call sites outside `version.rs` itself. `V20_2_0_7_SSE`'s only reference anywhere
is a tautological test asserting it equals `V20_2_0_7` (same bit pattern —
Skyrim/FO4 report the identical NIF version as Fallout 3+, there is no distinct
"SSE version").

## Verification

Independently re-derived the audit's claim: `grep -rn "V10_1_0_112\|V20_2_0_7_SSE\|V30_1_0_1" --include='*.rs' .` (excluding `target/`) confirmed all three
have zero references beyond their own definitions and (for `V20_2_0_7_SSE`) the
one tautological assert — exact match, no deviation.

Went further per-constant, since "zero grep hits" doesn't mean the same thing
for all three:

- **`V30_1_0_1`**: read `NiPersistentSrcTextureRendererData::parse`
  (`crates/nif/src/blocks/texture.rs`) — the `Platform`/`Renderer` field is
  decoded unconditionally, no version branch anywhere. Genuinely zero
  functional gate, confirmed by the code, not just by grep. The constant's own
  doc comment already concedes this ("no Redux-supported title reaches major
  version 30").
- **`V10_1_0_112`**: read `NiBlendInterpolator::parse`
  (`crates/nif/src/blocks/interpolator.rs`) — the modern-layout boundary it
  documents IS real, correct, and load-bearing (the #1508 fix), but the
  dispatch reaches it via the *adjacent* `V10_1_0_111`'s `<=` upper bound plus
  an `else` fallthrough — never a direct reference to `V10_1_0_112` itself.
  Materially different from the other two: the boundary is genuine, only the
  named constant was unused.

## Fix implemented

- Deleted `V20_2_0_7_SSE` and its tautological `version_ordering` assert — pure
  duplicate alias, zero functional or documentary value beyond what
  `V20_2_0_7`'s own (now slightly expanded) doc comment already states.
- Deleted `V30_1_0_1` — confirmed zero consumer, no near-term one planned (its
  own doc already said as much); matches the issue's own suggested-fix
  criterion ("keep only if a documented near-term consumer exists").
- **Kept `V10_1_0_112`**, but gave it a real, compiled reference instead of
  either deleting a genuinely-correct documented boundary or leaving it
  unreferenced: `NiBlendInterpolator::parse`'s `else` branch now carries
  `debug_assert!(version >= NifVersion::V10_1_0_112)` immediately before
  calling `parse_modern`. This makes the constant a first-class consumer of
  the exact boundary its doc comment describes, self-checks the `else`
  fallthrough's implicit assumption, and avoids duplicating the three-band
  dispatch logic just to name one more constant explicitly.

**SIBLING** (issue's own checklist item): re-scanned every `pub const V*` in
`version.rs` (45 total) for call-site presence — after this fix, zero orphans
remain in the file.

**TESTS** (issue's own checklist item): removing the tautological assert
doesn't reduce coverage — `version_ordering`'s two remaining assertions were
untouched, and no other test anywhere referenced either deleted constant
(confirmed by the same workspace-wide grep, post-fix, returning no hits).

Full workspace: `cargo test --no-fail-fast` 7049 passing, 0 failing (unchanged
count — no new tests added, none removed; the deleted tautological assert
lived inside an existing multi-assert test function, not its own `#[test]`).
