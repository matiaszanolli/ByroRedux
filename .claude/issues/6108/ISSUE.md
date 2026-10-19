# SKY-SD-03: Doc rot after #5408 and #2586

Labels: low,documentation,doc-rot,character,game:skyrim,legacy-compat

## Description
`#5408` changed the head-part fallback to race base parts for the NPC's gender, minus the types the NPC authors, plus the NPC's own parts, plus depth-first `HNAM` extras. Every comment, the phase label, the `info!` log line and two docs still describe the PNAM-only list, and `skip_missing_facegen`'s "only an NPC with neither source continues headless" is now "neither race head parts nor PNAM". Separately `byroredux/src/cell_loader/tree_lod.rs` (lines ~52-55) still calls #2586 an open problem; `cb5368348` closed it (all `.btr`/`.bto` tile from the `lodsettings\<ws>.lod` cell), and the tree ring's by-name selection no longer needs the justification.

Sites: `byroredux/src/npc_spawn/resumable/prebaked.rs:23-32` (`head_fallback` doc: "the NPC's own PNAM head-part meshes... authored order"), `:36-40`, `:48-52` (log line "assembling the head from N authored PNAM head part(s)"), `:75`, `:336-339`; `byroredux/src/npc_spawn/resumable/mod.rs:297` (phase label "PNAM head-part fallback") and `:1456`; `byroredux/src/npc_spawn.rs:1169`; `byroredux/src/player_body.rs:36`; `docs/engine/playable-vertical-slice.md:1305`; `.claude/commands/audit-skyrim/SKILL.md` Dim 3 ("loading the NPC's authored `PNAM` HDPT meshes in authored order"); and the `tree_lod.rs` paragraph.

## Impact
Misleads the next reader debugging a head (the log line says "authored PNAM" for a race-derived set).

## Related
SKY-SD-02 (the real behaviour of this path), EXT-D2-05 (a different doc-rot entry).

## Suggested Fix
Reword the six code sites and two docs to "head-part fallback (race base parts + NPC parts + extras)"; refresh the `tree_lod.rs` paragraph and the audit-skyrim SKILL Dim 3 sentence.

Source: docs/audits/AUDIT_SKYRIM_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Every other site that repeats the stale wording is updated in the same change
