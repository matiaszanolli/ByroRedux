**Severity**: MEDIUM · **Dimension**: CDB Material Correctness
**Location**: `crates/sfmaterial/src/reader.rs:153` (`ChunkType::from_raw`), `:257` (`UnknownClassFlags`), `:382,:447` (`UnsupportedBuiltin`); `crates/sfmaterial/src/types.rs` (`BuiltinType::from_u32`)
**Source**: `docs/audits/AUDIT_STARFIELD_2026-06-14.md` (SF-D3-02)

## Description
The CDB is a single flat chunk stream with no per-instance recovery: `parse()` walks the whole queue and propagates the first `Err` via `?`. One unrecognised FourCC chunk type, undocumented `BuiltinType` low-byte, or any class-flag bit outside `IsUser|IsStruct` aborts the ENTIRE parse, dropping all 1.44M materials. Vanilla parses cleanly today, but the format is content-addressed and version-evolving: a future patch or a Creations/DLC CDB adding one new reflection class flag or builtin would zero out Starfield materials wholesale rather than degrading the single affected class.

## Evidence
`from_raw`/`from_u32` and the `UnknownClassFlags` guard all `return Err(...)` (confirmed: `reader.rs:153,257,382,447`); the `while !state.chunks.is_empty()` loop uses `?` on every dispatch. **Panic impact disproven**: no `unwrap`/`expect`/`panic!`/`unreachable!` in the crate, and `load_starfield_cdb` (`asset_provider.rs:735-743`) catches the `Err` with `warn!` + Lambert fallback. Not a panic, not HIGH — a "lose everything on one unknown byte" brittleness.

## Impact
All-or-nothing CDB load. Zero impact on vanilla; on a future patch/DLC CDB with one unrecognised tag the whole material set silently falls back to keyword-guessed PBR with a single warn line — hard to diagnose because nothing names the offending class.

## Related
SF-D3-01 (#1289 Phase 2 — the consumer that this brittleness gates); SF-D3-03 (DLC CDB paths).

## Suggested Fix
Per-instance skip is non-trivial (positional instances desync on a wrong skip). Minimum viable: include the failing chunk index / class-flag raw value in the warn message (the `Error` variants already carry `index`/`raw`), and add a `cargo test` baseline pinning the vanilla class/flag/builtin set so a new tag is caught at test time, not silently at runtime.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Recovery stays at the CDB-parse boundary; the `.mat`→`Material` lookup (SF-D3-01) must not silently substitute keyword-guessed PBR for a *partially* parsed set without surfacing which classes were lost
- [ ] **TESTS**: A baseline test pins the vanilla `materialsbeta.cdb` class/flag/builtin tag set so a new tag fails at test time
