# #3476 — NIF-2026-08-27-D2-01: #2345 added 19 raw version comparisons in one parser without routing any through the named-helper surface

Source: `docs/audits/AUDIT_NIF_2026-08-27.md`
Filed: 2026-08-27 via `/audit-publish`
Labels: low, nif-parser, nif, tech-debt, bug

---

Audit: `docs/audits/AUDIT_NIF_2026-08-27.md` — Dimension 2 (Version Gating). Severity **LOW**. Game affected: all (maintainability).

## Location
`crates/nif/src/blocks/controller/sequence.rs:148-409` — 19 `stream.version() <op> NifVersion::V…` sites in `NiControllerSequence::parse`.

## Description
`crates/nif/src/version.rs:190-197` states the doctrine — "block parsers query *intent* instead of scattering raw `version < V10_1_0_0` literals" — and the `/audit-nif` Dim-2 checklist makes a new gate that hardcodes a literal *the regression*. The #2345 fix is byte-correct (every gate was re-verified against nif.xml `ControlledBlock` lines 1919-1950 and `NiSequence`/`NiControllerSequence` lines 4201-4231), but it introduced 19 raw comparisons in a single function and no helper.

Two of them — `sequence.rs:148` and `:153`, both `stream.version() <= NifVersion::V10_1_0_103` — are the *exact* predicate `NifVersion::has_keyframe_controller_data()` (`version.rs:262-264`) already implements, and that helper's doc comment explicitly enumerates the sibling fields sharing the boundary (`NiKeyframeController.Data`, `NiVisController.Data`, `NiAlphaController.Data`, …). `NiSequence`'s `Accum Root Name` / `Text Keys` pair sits on the same nif.xml boundary and was not added to that enumeration.

## Evidence
`grep -c "stream.version() [<>=]" crates/nif/src/blocks/controller/sequence.rs` → **19** (re-verified at publish time). `impl NifVersion`'s live helper surface is exactly the 9 methods at `version.rs:204-297`, none of which this parser calls.

## Impact
None at runtime. The cost is that the next person changing the 10.1.0.10x boundaries has to find 19 sites in one function rather than one helper, and that `has_keyframe_controller_data`'s doc now under-describes the fields on its own boundary.

## Related
#1511 / #1840 / #1897 (the "a helper with no call site is dead code" lesson — this is the mirror case: call sites with no helper); #2345.

## Suggested Fix
Add `NifVersion::has_ni_sequence_prologue()` (`self <= V10_1_0_103`) and `has_controller_sequence_fields()` (`self >= V10_1_0_106`) *with* these call sites, and extend `has_keyframe_controller_data`'s doc enumeration to name the `NiSequence` pair.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other block parsers on the 10.1.0.10x boundary that hardcode the same literals)
- [ ] **TESTS**: A regression test pins this specific fix (the new helpers' boundary behaviour, so the refactor is byte-neutral)
