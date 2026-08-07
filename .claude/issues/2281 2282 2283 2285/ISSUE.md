# Issues 2281, 2282, 2283, 2285

## #2281 — NIF-D2-02: Bare bsver literals in NifVariant::detect / sequence.rs
**Severity**: LOW · **Labels**: bug, nif-parser, low, tech-debt, nif
**Location**: `crates/nif/src/version.rs:529-548` (`NifVariant::detect`);
`crates/nif/src/blocks/controller/sequence.rs:177`

`NifVariant::detect`'s match arms hardcode `34`/`83`/`100`/`130`/`155`/`170`
instead of the named `bsver::*` constants defined a few dozen lines above in
the same file, violating the file's own doc-mandated named-constant
doctrine. `sequence.rs:177` uses `bsver > 0` instead of the crate idiom
`bsver > bsver::PRE_BETHESDA` used at 3 other sites.

**Fix**: Rewrite `detect()`'s match arms to reference `bsver::*` constants;
change `sequence.rs:177` to use `bsver::PRE_BETHESDA`. No behavior change
expected (values already agree) — confirmed by existing `detect_*` /
`bsver_values()` tests.

## #2282 — NIF-D6-01: parse_particle_system's modifier_refs bypasses allocate_vec
**Severity**: LOW · **Labels**: bug, nif-parser, low, tech-debt, nif
**Location**: `crates/nif/src/blocks/particle.rs:1130-1139`

Hand-rolled `check_alloc` + `reserve_exact` + push-loop pattern instead of
the crate-standard `NifStream::allocate_vec` used everywhere else
(including all 3 `ragdoll.rs` sites).

**Fix**: Replace with `stream.allocate_vec::<BlockRef>(num_modifiers)?` +
push loop, matching `read_block_ref_list` and other bulk-ref sites.

## #2283 — NIF-D4-01: BsTriShapeKind::LOD cutoffs unreachable (regression of #1207)
**Severity**: MEDIUM · **Labels**: bug, nif-parser, medium
**Location**: `crates/nif/src/blocks/mod.rs:453-456`;
`crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:568-591`;
`crates/nif/src/import/mesh/bs_tri_shape.rs:204-207`;
`crates/nif/src/import/walk/mod.rs:477-500,1113-1138`

`"BSMeshLODTriShape"` parses via `BsTriShape::parse_lod()`, producing
`BsTriShapeKind::LOD{lod0,lod1,lod2}` — but the dispatcher immediately
overwrites it with `.with_kind(BsTriShapeKind::MeshLOD)`, discarding the
cutoffs in the same expression on every real parse. `extract_bs_tri_shape`
matches on the now-unreachable `LOD` variant to populate `bs_lod_cutoffs`.
`"BSLODTriShape"` (`NiLodTriShape`) carries its own `lod0_size`/`lod1_size`/
`lod2_size` fields that are never threaded through at all — the walker
unwraps to the inner classic `NiTriShape` and uses the `ni_tri_shape.rs`
extractor, which hardcodes `bs_lod_cutoffs: None`.

The #1207 regression test bypasses the real dispatcher (hand-builds a
`BsTriShape` fixture with `kind: LOD{...}` directly) so it can't catch this.

**Fix**: Drop the `.with_kind(MeshLOD)` override (or add a
`MeshLOD{lod0,lod1,lod2}` variant) so `BSMeshLODTriShape`'s parsed cutoffs
survive to import; thread `NiLodTriShape`'s own lod0/1/2 sizes into
`bs_lod_cutoffs` from its walker branch. Rewrite the #1207/#988 tests to
drive the real block dispatcher on synthetic `"BSLODTriShape"` /
`"BSMeshLODTriShape"` blocks.

## #2285 — NIFAL-D6-07: finish_trimesh validates merged total, not per-buffer range
**Severity**: MEDIUM · **Labels**: bug, nif-parser, medium
**Location**: `crates/nif/src/import/collision/shape.rs:591-605`
(`finish_trimesh`), consumed by `resolve_compressed_mesh` (484-582) and
`resolve_tri_strips_data_refs` (361-407)

`finish_trimesh`'s bounds check validates each index against the *final
merged* vertex count, not the sub-buffer it was decoded from. A
corrupt/truncated NIF whose `CmsBigTri` index exceeds `big_verts.len()` but
is still `< all_verts.len()` (because later chunks pushed enough vertices)
passes unchanged — silently splicing unrelated chunk geometry instead of
being dropped as corrupt.

**Fix**: Track each sub-buffer's own vertex-count range (or validate/retain
each source's own index slice before merging into `all_indices`) so
`finish_trimesh`'s global check becomes belt-and-suspenders, not the only
guard.
