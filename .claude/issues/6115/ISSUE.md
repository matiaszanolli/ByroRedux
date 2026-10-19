# NIF-SD-02: Only the synchronous REFR loader looks at `NifScene::truncated`; the streaming worker and the NPC loader cache partial scenes silently

Labels: low,nif-parser,nif,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6115

**Severity**: LOW · **Dimension**: Stream Position (consumer tolerance) · **Games**: all; vanilla content measures 0 truncations, so this is mod content

## Location
`byroredux/src/streaming/pre_parse.rs` (`parse_one_nif`, worker), `byroredux/src/scene/nif_loader.rs` (`parse_import_and_merge`, NPC / loose loader) versus `byroredux/src/cell_loader/references/import.rs` (`parse_and_import_nif`)

## Description
`NifScene` carries `truncated`, `dropped_block_count`, `recovered_blocks` and `recovered_by_guess`, documented as "consumers that can tolerate partial geometry can ignore it" (`crates/nif/src/scene.rs`). `parse_and_import_nif` logs a warning when `truncated` is set and a second warning for a zero-contribution import. `parse_one_nif` does neither, and `parse_import_and_merge` (every NPC skeleton, body, head and armour NIF) logs only the zero-mesh case. The truncated scene is then cached as a positive entry, so there is no retry and no later signal. The scene's `root_index` heuristic may also pick a subtree when truncation hits before the root, so a partial skeleton can import as a smaller skeleton rather than failing.

## Evidence
A grep of `byroredux/src` for `s.truncated` / `recovered_blocks` / `dropped_block_count` finds the truncation check in `references/import.rs` only. `StreamingTelemetry` has no parser-health counter.

## Impact
A mod NIF that truncates mid-file renders partially (exterior statics) or yields a mis-bound actor (NPC skeleton) with no log line naming the file. Observability only; nothing crashes.

## Related
#393, #568 (closed; corpus-side counterparts).

## Suggested Fix
Route the three consumers through one helper that warns once per path on `truncated` / `recovered_blocks > 0` and counts them in the streaming telemetry.

Source: docs/audits/AUDIT_NIF_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
