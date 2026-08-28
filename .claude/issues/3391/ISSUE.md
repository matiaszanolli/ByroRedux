# Issue #3391 — SF-2026-08-27-D2-01: canonical_mesh_path panics on a non-ASCII BSGeometry mesh name (char-boundary slice)

Filed: 2026-08-27 by `/audit-publish` from `docs/audits/AUDIT_STARFIELD_2026-08-27.md`

Labels: `high,bug,nif-parser,nif,import-pipeline,game:starfield,legacy-compat`

> Immutable snapshot of the issue as filed (TD10-001 / #1156).
> GitHub is authoritative for current state: `gh issue view 3391 --json state`.

---

Found by `/audit-starfield` — [`docs/audits/AUDIT_STARFIELD_2026-08-27.md`](docs/audits/AUDIT_STARFIELD_2026-08-27.md), Dimension 2 (BSGeometry mesh extraction).

- **Severity**: HIGH
- **Location**: `crates/nif/src/import/mesh/bs_geometry.rs:48-49` (`canonical_mesh_path`), call site `:134` (`extract_bs_geometry`)
- **Status**: NEW — introduced by `61520a39` (2026-08-26, the #2361 fix), *after* the 2026-08-24 audit found this dimension clean.

## Description

`canonical_mesh_path`'s `has_tail` test slices the `&str` by byte range:

```rust
let has_tail = mesh_name.len() > TAIL.len()
    && mesh_name[mesh_name.len() - TAIL.len()..].eq_ignore_ascii_case(TAIL);
```

`mesh_name.len() - 5` is a **byte** index. If the last five bytes straddle a multibyte UTF-8 scalar, `Index<Range<usize>> for str` panics.

The `has_head` test two lines above operates on `mesh_name.as_bytes()` and is safe — the tail test silently switched representation. The commit's own doc comment states the helper mirrors `normalize_mesh_path`'s technique, and `byroredux/src/asset_provider/archive.rs:96-118` is byte-slice-only end-to-end and therefore panic-free. The reimplementation deviated at exactly this one line.

## Evidence

1. `mesh_name` is untrusted archive input decoded **lossily**. `BSGeometryMesh::parse` reads it via `stream.read_sized_string()` (`crates/nif/src/blocks/bs_geometry.rs:241`), which falls back to `String::from_utf8_lossy` (`crates/nif/src/stream.rs:632-641`). Each invalid byte becomes a 3-byte U+FFFD. No ASCII validation exists on this path.

2. Panic reproduced against a byte-identical copy of the function (`rustc -O`, standalone) — by the dimension agent, then independently re-run by the audit orchestrator:

```
len=12 boundaries_ok=false
panicked: start byte index 7 is not a char boundary; inside 'е' (bytes 6..8)   // "модель"
panicked: start byte index 1 is not a char boundary; inside '\u{fffd}' (0..3)  // from_utf8_lossy(&[0xFF,0xFF])
ascii: geometries\abc123.mesh                                                  // ASCII path correct
```

3. It does **not** require corrupt data. Valid non-ASCII suffices: `"модель"` is 12 bytes with boundaries at 0,2,…,12; `12 - 5 = 7` is mid-char.

4. **The panic is not caught.** `extract_bs_geometry` is reached from `import_nif_scene_with_resolver` (`crates/nif/src/import/walk/mod.rs:554`, `:1231`), called on the **main thread** from `byroredux/src/scene/nif_loader.rs:257`, `byroredux/src/cell_loader/placement_lod.rs:489`, `byroredux/src/cell_loader/object_lod.rs:279`, `byroredux/src/cell_loader/terrain_lod_btr.rs:233`. The streaming worker's `catch_unwind` guards (`byroredux/src/streaming.rs:1118`, `:1153`) wrap only `parse_nif` plus the satellite walkers — mesh extraction is outside both. `panic = "unwind"` is workspace-wide (`Cargo.toml:254`).

5. Reachability is gated only on a resolver being present (`bs_geometry.rs:123`), which the engine always supplies via `impl MeshResolver for TextureProvider` (`byroredux/src/asset_provider/texture.rs:81-85`).

### Attempts to disprove (all failed)

No upstream ASCII/UTF-8 filter on `mesh_name`; no length or charset cap in `read_sized_string`; no `catch_unwind` on the main-thread import path; `has_head`'s byte-slice does not protect the tail test — they are independent `&&` chains and the tail test evaluates for every name >= 6 bytes regardless of head.

## Impact

Hard engine crash during Starfield cell load or loose-NIF load for any `BSGeometry` whose external `.mesh` name is non-ASCII or invalid UTF-8 — the latter also covers a truncated/misparsed `.nif` whose `u32` length prefix lands on arbitrary bytes.

Vanilla Starfield is unaffected (all sampled names are bare 20-hex ASCII stems), so blast radius is mods, authoring-tool output, localized paths and corrupt archives. The regression is asymmetric: pre-`61520a39` this input produced a silent resolve miss; post-fix it terminates the process.

## Suggested Fix

Make the tail test byte-wise, matching the head test three lines above:

```rust
let has_tail = bytes.len() > TAIL.len()
    && bytes[bytes.len() - TAIL.len()..].eq_ignore_ascii_case(TAIL.as_bytes());
```

Behaviour-preserving for all six existing `canonical_mesh_path_tests` (all ASCII). Add a seventh asserting a non-ASCII name returns rather than panics.

## Related

Introduced fixing #2361; the composition it replaced dates to #1292; miss-path logging is #2357. Same-class precedent: #854 (worker-thread panic guards), the `crates/bsa/src/ba2.rs:775` `catch_unwind` + its presence test at `:1685`.

**Same defect class** as the sibling finding `SF-2026-08-27-D1-02` (byte-slicing a `&str` at a computed offset, in `crates/bsa/src/ba2.rs` test infra). Two unrelated commits the same week; a `clippy::string_slice` lint would catch both.

## Coverage caveat from the audit

No dimension re-measured the `.mesh` name distribution across the Starfield mesh archives. The "all vanilla names are bare 20-hex ASCII stems" premise — which is what bounds this finding to non-vanilla content — is taken from `61520a39`'s commit message, not independently verified. **A corpus scan for any non-ASCII `BSGeometryMeshKind::External` name would either confirm vanilla safety or escalate this finding to CRITICAL.**

## Completeness Checks
- [ ] **SIBLING**: Same byte-slice pattern checked in related files (`crates/bsa/src/ba2.rs:1686`/`:1718` are known siblings; sweep for other `[..n]` on `&str`)
- [ ] **CANONICAL-BOUNDARY**: fix stays inside the NIF import path; no per-game logic pushed into shaders/renderer
- [ ] **TESTS**: A regression test pins the non-ASCII case specifically
