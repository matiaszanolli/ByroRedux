# SK-D6-LOW: ESM CELL-meta hardening — thread-local leak + missing FULL consumer + IMGS dispatch

## Finding: SK-D6-LOW (bundle of SK-D6-NEW-01/02/03)

- **Severity**: LOW (all items)
- **Source**: `docs/audits/AUDIT_SKYRIM_2026-04-24.md`

These three are all in the CELL-meta consumer surface; consider folding into the existing #566 (LGTM lighting-template fallback) follow-up since LTMP, FULL, and IMGS share the same expansion pattern.

## SK-D6-NEW-01: is_localized_plugin thread-local leaks across overlapping ESM parses

**Location**: [crates/plugin/src/esm/records/common.rs:26](crates/plugin/src/esm/records/common.rs#L26) (setter); set/clear pair at [records/mod.rs:251 / 477](crates/plugin/src/esm/records/mod.rs#L251).

`set_localized_plugin` writes to a `thread_local!` cell. `parse_esm` sets it from the file's TES4 flag at line 251 and clears at 477. Walking two ESMs concurrently on the same thread (or panicking inside one walk) leaves the flag in an undefined state — the next parse on this thread reads FULL/DESC of a non-localized FNV plugin through the lstring branch, returning `<lstring 0x…>` placeholders.

**Fix**: replace the thread-local with a value passed down through `parse_esm` → `parse_*` closures (or a `ParseCtx` struct). Or wrap the set/clear pair in a `Drop`-guard struct so panics can't leak. Add a panic-during-parse regression test.

## SK-D6-NEW-02: Cell walker FULL is read as raw zstring, not lstring (and is not even consumed)

**Location**: [crates/plugin/src/esm/cell.rs](crates/plugin/src/esm/cell.rs) — `b"FULL"` is **not matched** in the CELL sub-record loop. `CellData` has no `display_name` field.

Skyrim cells DO ship FULL (e.g. WhiterunBanneredMare's FULL = "The Bannered Mare"). The display name is never indexed; console / UI commands keying off cell name see only `editor_id`. Doesn't block cell rendering — surfaces only on future "Show: 'Bannered Mare'" UI commands (M48-class).

#348 (closed) added the lstring helper but the cell walker doesn't call it.

**Fix**: add `pub display_name: Option<String>` to `CellData`, consume `b"FULL"` via `read_lstring_or_zstring` (handles both Localized FULL u32-into-STRINGS index and inline FNV-style zstring with the same helper).

## SK-D6-NEW-03: IMGS / IMSP / IMAD imagespace records dropped at the catch-all skip

**Location**: [crates/plugin/src/esm/records/mod.rs:435](crates/plugin/src/esm/records/mod.rs#L435) (`_ =>` arm calls `skip_group`); CELL parser at cell.rs:744 reads XCIM as a u32 form-ref but no consumer can resolve the target.

`grep -rn 'b"IMGS"|b"IMSP"|b"IMAD"' crates/plugin/` returns zero hits. CELL.XCIM stores an imagespace FormID (per-cell tone-map / colour-grading LUT). Skyrim ships ~1k IMGS entries; almost every Solitude / Whiterun interior overrides the worldspace default. Without an IMGS index a future per-cell HDR-LUT consumer cannot resolve XCIM.

No effect today. Becomes a quality blocker the moment the composite HDR pipeline grows a per-cell-LUT input.

**Fix**: add `b"IMGS" => extract_records(...)` arm following the LGTM (mod.rs:384) shape; parser stub mirroring `parse_lgtm` (EDID + scalar floats: brightness, saturation, tint, fade times). Real IMAD modifier graph deferred to M48.

## Related

- #566 (open): LGTM lighting template fallback — same CELL-meta consumer surface; consider folding.
- #348 (closed): Skyrim FULL lstring — added the helper; this finding is the missing consumer site.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Audit if other localized records (BOOK.DESC, MGEF.DNAM, SPEL.DESC) are also missed at their consumer sites the same way FULL is.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: For SK-D6-NEW-01: if ParseCtx replaces the thread-local, verify it's not Send-violating in any concurrent code path.
- [ ] **FFI**: N/A
- [ ] **TESTS**: SK-D6-NEW-01 — panic-during-parse test. SK-D6-NEW-02 — assert CellData.display_name == "The Bannered Mare" for vanilla Skyrim cell. SK-D6-NEW-03 — assert vanilla Skyrim.esm IMGS count > 0 in EsmIndex.

_Filed from audit `docs/audits/AUDIT_SKYRIM_2026-04-24.md`._
