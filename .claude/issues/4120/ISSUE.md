# SPT-2026-09-11-D1-01: three source files still describe SpeedTree geometry as living in a deferred 'geometry tail' that #3808 determined does not exist

URL: https://github.com/matiaszanolli/ByroRedux/issues/4120
Labels: documentation, low, terrain-exterior, speedtree, doc-rot

- **Severity**: LOW
- **Dimension**: Walker Byte-Accounting / Placeholder Fallback (doc-rot, spans both) (audit-speedtree)
- **Location**: `crates/spt/src/parser.rs:1-25` (module doc); `crates/spt/src/import/mod.rs:9-48` (module doc, "SpeedTree Phase 2" section); `byroredux/src/cell_loader/references/import.rs:432-437,455-456,556-557`
- **Status**: NEW (residual of `#3808`, closed 2026-09-07 by `75ea6533` — that commit corrected `crates/spt/src/scene.rs` and `docs/engine/exal-trees.md` but did not sweep the source-code doc comments in the other files that describe the same now-superseded framing)

**Description**

`#3808`'s research spike (see `crates/spt/docs/format-notes.md`, "2026-09-07 — Phase 2.1 geometry-tail dissection") established that what the walker calls `tail_offset` is not the start of a binary geometry section: it is a desync point inside the *same* TLV parameter stream, past `parser::TAG_MAX`, and no `.spt` in the 159-file corpus is large enough to hold baked branch/frond/leaf-card geometry at all (largest file 8,793 B, under the cost of 274 vertices of position+normal+UV for the *entire* file). `crates/spt/src/scene.rs`'s `tail_offset` doc and `docs/engine/exal-trees.md` were corrected to say so. Three other locations that make the identical claim were not:
- `crates/spt/src/parser.rs`'s module-level doc comment (predating and unchanged by `#3808`) still reads: "Stops cleanly when the next tag is out of range — that's the binary geometry tail (Phase 1.3 follow-up)" and "The peeked u32 isn't in `[TAG_MIN, TAG_MAX]` — geometry tail" — describing the exact claim `scene.rs`, twelve lines away in the same crate, now explicitly retracts.
- `crates/spt/src/import/mod.rs`'s module-level doc comment still has a whole section, "SpeedTree Phase 2 (planned, no ROADMAP row — gated by `crates/spt/docs/format-notes.md`'s 'Geometry tail' section)", whose first bullet is "Decode the geometry tail past `tail_offset` → real branch / frond meshes with the bark texture" — the literal thing `#3808` found is not possible to do because there is no geometry there.
- `byroredux/src/cell_loader/references/import.rs` has three separate comments carrying the same premise (lines 434, 455-456, 556-557).

**Evidence**

`scene.rs`'s own corrected doc explicitly names this exact failure mode: "`tail_offset` used to be documented as 'where the binary geometry tail begins'. The 2026-09-07 dissection (#3808) measured that claim and it does not hold" (`scene.rs:8-10`). The three locations above are precisely instances of the documentation `scene.rs` is contrasting itself against, still standing.

**Impact**

None today — no code branches on "is this the geometry tail", the placeholder importer is unconditional either way, and nothing has attempted to write a Phase-2 decoder against these comments yet. The risk is forward-looking and specific: `format-notes.md`'s own 2026-09-07 entry says the *next* concrete parser task is "raising `TAG_MAX` and dictionarying the 14000-22000 bands... with the desync fixed first" — ordinary TLV work, not geometry decoding. A contributor who starts from `import/mod.rs`'s "Phase 2" section or `import.rs`'s three comments instead of `format-notes.md` would look for a geometry layout that the same audit cycle's own research proved does not exist.

**Related**

#3808 (closed, correctly, but the sweep was scoped to `scene.rs` + `exal-trees.md` only); companion finding SPT-2026-09-11-D1-02 (the desync itself, filed separately).

**Suggested Fix**

Reword `parser.rs`'s module doc to match `scene.rs`'s (a TLV stream that continues past `tail_offset`, capped by `TAG_MAX`, not a distinct geometry section). Retarget `import/mod.rs`'s "Phase 2" section at the three actual re-scoped directions `exal-trees.md` §3/§10 now lists (generate branch/leaf geometry from authored parameters / keep billboards permanently / source real tree meshes elsewhere) instead of "decode the geometry tail". Update the three `import.rs` comments similarly. All four are comment-only changes.

## Completeness Checks
- [ ] **SIBLING**: Re-grep the whole repo for "geometry tail" / "geometry-tail" after the fix to confirm no fifth occurrence survives

Source: `docs/audits/AUDIT_SPEEDTREE_2026-09-11.md`
