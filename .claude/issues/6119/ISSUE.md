# NIF-SD-05: `pre_parse_one` runs `extract_mesh` outside every panic guard, so a reader panic discards the whole cell's payload and replays unguarded on the main thread

Labels: low,nif-parser,nif,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6119

**Severity**: LOW (defense-in-depth) · **Dimension**: Allocation Hygiene / Stream Position (failure containment) · **Games**: all

## Location
`byroredux/src/streaming/pre_parse.rs` (`pre_parse_one`: `ctx.mesh_resolver.extract_mesh(&input.key)` runs outside `parse_one_nif`'s `catch_unwind`; cell-level guard `pre_parse_cell_panic_safe`)

## Description
A panic inside `parse_one_nif` becomes a `None` result and then a negative registry entry (#854), and the precombine job honours that (#5522) so the panic is not replayed. The archive extraction that feeds it is not inside that guard. A panic there propagates through `in_place_scope_fifo`, is caught only by `pre_parse_cell_panic_safe`, and produces an empty `parsed` map: every sibling NIF the cell had parsed is discarded and nothing is recorded as failed. The main thread then falls back to `synth_child.rs`, which calls the same `extract_mesh` with no `catch_unwind` - the replay #5522 closed for the parse step.

## Evidence
Control flow above (verified at publish time: `extract_mesh` at the top of `pre_parse_one`, `catch_unwind` inside `parse_one_nif`). The reader returns `io::Result` by design, so this needs an archive-reader bug to trigger (`/audit-parsers` territory, e.g. #5008); no trigger could be constructed statically.

## Impact
Latent. With a reader panic bug, one corrupt mesh entry becomes a process abort on the next load of its cell instead of a missing model.

## Related
#854, #5522, #5008.

## Suggested Fix
Move the `extract_mesh` call inside the per-NIF `catch_unwind` in `parse_one_nif` (or wrap it separately) so extraction failure takes the same negative-entry path.

Source: docs/audits/AUDIT_NIF_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
