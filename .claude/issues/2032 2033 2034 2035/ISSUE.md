# Batch: 2032, 2033, 2034, 2035

Source: `docs/audits/AUDIT_PERFORMANCE_2026-07-16.md`

## #2032 — PERF-D8-01: BSGeometryMeshData::parse skin-weight loop bypasses the bulk read_pod_vec path its own doc comment specifies
- Severity: MEDIUM · Dimension: NIF Parse
- Location: `crates/nif/src/blocks/bs_geometry.rs:466-489` (loop), struct + doc comment
  at `:293-305`
- `BoneWeight` is `#[repr(C)]` + POD (`unsafe impl AnyBitPattern`), doc comment says
  parse via `read_pod_vec::<BoneWeight>`, but actual body does `allocate_vec` +
  per-element loop of two `read_u16_le()` calls — the pre-#873/#1589 pattern. Sibling
  reads (`meshlets`, `cull_data`) in the same function already use the bulk path.
- Suggested fix: `stream.read_pod_vec::<BoneWeight>(outer_len * weights_per_vert)?`
  then `.chunks_exact(weights_per_vert).map(|c| c.to_vec()).collect()` — must use
  `outer_len * weights_per_vert`, NOT `n_total_weights`, to byte-for-byte preserve
  today's truncating-division stream-position behavior.
- Completeness: SIBLING (verify byte-identical stream position/output vs
  meshlets/cull_data path), UNSAFE (confirm BoneWeight POD invariant unchanged),
  TESTS (wall-clock/call-count regression, dhat allocation bounds can't catch this).
- Domain: nif → `byroredux-nif`

## #2033 — PERF-D1-2026-07-16-01: M42 AI-package systems allocate a fresh per-frame decision Vec
- Severity: LOW · Dimension: CPU Hot Paths
- Location: `byroredux/src/systems/{wander,travel,follow,escort,guard,patrol}.rs`
  (one `Vec::new()` each) and `sandbox.rs:152,169,171` (two Vecs + a HashMap)
- Each of the 7 M42 AI-package runtimes allocates a fresh `Vec::new()` per
  invocation instead of the closure-captured persistent-scratch pattern
  `make_animation_system`/`make_billboard_system` already use. Opt-in only — all 7
  gated behind per-behavior env vars in `boot.rs:721-754`, never in the default
  scheduler.
- Suggested fix: convert each to a `make_*_system()` factory capturing persistent
  scratch reused via `clear()`.
- Domain: binary → `byroredux`

## #2034 — PERF-D1-2026-07-16-02: collect_lights recomputes gi_priority_score on both sides of every sort comparison
- Severity: LOW · Dimension: CPU Hot Paths
- Location: `byroredux/src/render/lights.rs:206-207`
- Comparator recomputes `gi_priority_score` on both sides of every comparison
  instead of precomputing once per element (Schwartzian transform). Compute-only;
  point-light counts are small (streaming-RIS-capped, typically <50).
- Suggested fix: precompute `gi_priority_score` once per light into a parallel
  array/tuple before sorting, if this path is ever revisited for a larger light
  count. Issue itself says "not worth a change unless a hundreds-of-lights cell
  ever materializes."
- Completeness: TESTS N/A — informational optimization only.
- Domain: binary → `byroredux`

## #2035 — MEM-D3-02: Stale MeshRegistry doc comment claims freed-slot reuse that doesn't exist
- Severity: LOW (documentation) · Dimension: GPU Memory Pressure
- Location: `crates/renderer/src/mesh.rs:39-43` vs upload path `:295-314`, drop doc
  `:587-589`
- `MAX_MESH_SLOTS` doc claims "re-uses freed slots via drop-and-push"; actual
  upload path is grow-only, contradicting the correct `drop_mesh` doc 550 lines
  away ("Handles stay stable... Re-using a handle would... produce silent data
  corruption").
- Suggested fix: fix `MAX_MESH_SLOTS` doc to match `drop_mesh`'s correct statement.
- Completeness: TESTS N/A (doc-only fix).
- Domain: renderer → `byroredux-renderer`
