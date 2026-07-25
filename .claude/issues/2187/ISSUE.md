# FO3-D4-NEW-01: `object_lod.rs` module doc still frames the FO3/FNV `.lod` scheme as if it works, contradicting the now-fixed #2086 gate

**GitHub Issue**: #2187
**Source Report**: `docs/audits/AUDIT_FO3_2026-07-25.md` — Dimension 4 (FO3 Cell Loading End-to-End)
**Severity**: LOW
**Labels**: low, legacy-compat, documentation

## Location
`byroredux/src/cell_loader/object_lod.rs:16-18`

## Description
The module doc comment reads: "**Oblivion / FO3 / FNV**: a different scheme
entirely — per-cell `DistantLOD\*.lod` placement lists instancing `_far.nif`
meshes, handled by the sibling [`super::placement_lod`] module (#1726)." This
groups FO3/FNV with Oblivion as if the sibling module actively provides
object LOD for all three, when #2086 established the sibling module's gate
now excludes FO3/FNV entirely (zero vanilla `.lod` files exist for those
titles) — for FO3/FNV this is a documented no-op, not "handled."

This finding is a completeness-check follow-up flagged inside the (closed)
#2086 fix — "SIBLING: object_lod.rs's Skyrim+/FO4 `.bto` path... confirm
consistent framing" — that was left unchecked/undone.

## Evidence
- `byroredux/src/cell_loader/object_lod.rs:16-18` — module doc still lists
  "Oblivion / FO3 / FNV" together with no hedge.
- `byroredux/src/cell_loader/placement_lod.rs:305-307` —
  `placement_lod_supported` returns `true` only for `GameKind::Oblivion`,
  with a doc comment directly above it citing FO3-D4-01/#2086 and stating
  FO3/FNV ship zero `distantlod\*.lod` files in any vanilla archive.

## Impact
Documentation-only; no functional bug (the gate itself is correct). A
future contributor reading `object_lod.rs` first (its module doc is the
more prominent LOD-scheme overview comment) would incorrectly believe
FO3/FNV get object LOD via `placement_lod`.

## Related
- #2086 (closed, left this exact follow-up unchecked)

## Suggested Fix
Update the `object_lod.rs` module comment's third bullet to note that
Oblivion is the only title with real `.lod` content; FO3/FNV fold landmark
LOD into the terrain-LOD block tree with no distant-object scheme currently
wired.

## Validation
Path premise re-verified directly against current code before filing:
- `object_lod.rs:16-18` confirmed still reads as described.
- `placement_lod.rs:305-307` confirmed gates on `GameKind::Oblivion` only.
- No open issue duplicates this (fresh `gh issue list` pull, 83 open/closed
  issues checked); #2086 confirmed CLOSED and is the issue this doc comment
  should have been synced against.
