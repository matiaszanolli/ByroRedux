# NIF-D5-01: Starfield BSGeometry block undispatched — 30% of every SF NIF lost

URL: https://github.com/matiaszanolli/ByroRedux/issues/708
Labels: enhancement, nif-parser, critical

---

## Severity: CRITICAL

## Game Affected
Starfield (all mesh archives)

## Location
- `crates/nif/src/blocks/mod.rs` — no dispatch arm; falls to `_ => Ok(NiUnknown)` at line 824

## Description
nif.xml line 3855 describes `BSGeometry` as the post-`NiGeometry` replacement for Bethesda 20.2.0.7+ NIFs ("NiGeometry was changed to BSGeometry…to add data exclusive to BSGeometry"). In Starfield's NIFs the type surfaces as a concrete top-level block — it is the dominant geometry container, replacing `BSTriShape`/`BSSubIndexTriShape` from FO4. Without a parser the entire mesh body is discarded.

## Evidence
2026-04-26 corpus sweep:
- `Starfield - Meshes01.ba2` — **190,549 occurrences (24.74% of every block)**
- `Starfield - FaceMeshes.ba2` — **13,713 occurrences (14.27% of blocks)**

100% of Starfield character / weapon / clutter mesh content lands in this block. Per `crates/nif/examples/d5_unk_ba2.rs` histogram.

## Impact
Starfield will never render geometry without this. Project compatibility note already accepts SF clean-parse rate at 0.80%; this is the single block holding that number down. Lifting just `BSGeometry` (+ paired `SkinAttach` per #NIF-D5-02 and `BoneTranslations` per #NIF-D5-08) lifts SF clean-parse rate from 0.80% toward 70%+.

## Suggested Fix
Author a `BSGeometry` parser. Layout in Starfield is a superset of `BSTriShape` with external mesh references — vertex/index data is in a separate file; the .nif holds bounds, material refs, segment table, and a path to the external mesh file. nif.xml's `BSGeometry` description is a starting point; the actual SF wire layout will need disassembly of an SF-era NIF via `crates/nif/examples/trace_block.rs`.

This is a multi-session task, not a one-shot PR. Bundle with NIF-D5-02 (`SkinAttach`) and NIF-D5-08 (`BoneTranslations`) — the three blocks form the SF skinned-geometry triple.

## Related
- Audit: docs/audits/AUDIT_NIF_2026-04-26.md (NIF-D5-01)
- Bundle: NIF-D5-02 SkinAttach, NIF-D5-08 BoneTranslations
- Related: NIF-D5-09 BSFaceGenNiNode (also Starfield)

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: BSTriShape, BSSubIndexTriShape parsers reviewed for shared layout patterns
- [ ] **DROP**: N/A (parser-only, no Vulkan objects)
- [ ] **LOCK_ORDER**: N/A (parser-only)
- [ ] **FFI**: N/A
- [ ] **TESTS**: Per-archive corpus regression in `dispatch_tests.rs`; byte-exact unit test from a captured SF NIF
- [ ] **CORPUS**: Reproduce zero NiUnknown for `BSGeometry` on `Starfield - Meshes01.ba2`
