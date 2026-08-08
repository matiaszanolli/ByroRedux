# NIF-D2-2026-08-07-02: audit-nif SKILL checklist's live-helper enumeration is missing has_skin_data_vertex_weights_flag

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2529
**Finding ID**: NIF-D2-2026-08-07-02

**Severity**: LOW
**Dimension**: Version Gating
**Game Affected**: None functionally — audit tooling text only
**Location**: `.claude/commands/audit-nif/SKILL.md:72`
**Status**: NEW

## Description
The SKILL's Dimension 2 checklist enumerates 8 live `NifVersion` helper names. `version.rs` today carries a 9th, `has_skin_data_vertex_weights_flag` (added by `#2168`, gating `NiSkinData.Has Vertex Weights` at `since="4.2.1.0"`), with a live call site at `blocks/skin.rs:113`. It was a pure addition (not a pruning), so it wasn't caught by the checklist's "pruned twice" framing when the checklist was last refreshed.

## Evidence
Confirmed directly: `version.rs:243` defines `has_skin_data_vertex_weights_flag`, `blocks/skin.rs:113` calls it live (`if stream.version().has_skin_data_vertex_weights_flag()`), but the SKILL.md checklist's enumerated helper list omits it.

## Impact
None on runtime correctness — the helper is correctly implemented, tested, and has a live consumer. Only affects future auditors' completeness-checking against the checklist text.

## Related
`#2168` (added the helper); companion to the sibling NifVariant-doc finding filed alongside this one (same root cause — audit-tooling text lagging a source addition).

## Suggested Fix
Add `has_skin_data_vertex_weights_flag` to the enumerated list in `.claude/commands/audit-nif/SKILL.md:72`.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
