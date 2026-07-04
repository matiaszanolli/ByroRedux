# #1827 — FO4-D4-02: Starfield BSGeometry leaves per-vertex bone indices/weights empty (informational, out of FO4 scope)

**Severity**: LOW · **Domain**: nif (`byroredux-nif`)
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:169-173`

For the FO4 path (`BsTriShape`) skinned bone indices/weights honor the packed
layout correctly. The Starfield `BSGeometry` sibling resolves the skin chain
for bind matrices but intentionally leaves per-vertex bone indices/weights
empty. This is BSVER 172 (Starfield), not BSVER 130 (FO4) — out of the
originating audit's FO4 scope, filed for tracking since it's a real, confirmed
gap.

Impact: Starfield skinned meshes render in bind pose. Zero FO4 impact.

Suggested fix: Track as Starfield skinning work (separate milestone) — decode
the packed `BSGeometry` per-vertex bone index/weight channel analogous to the
FO4 `BsTriShape` path.

---

# #1830 — SF2-03: BSGeometryMesh tri_size/num_verts hints parsed then never validated against resolved geometry

**Severity**: LOW · **Domain**: nif (`byroredux-nif`)
**Location**: `crates/nif/src/blocks/bs_geometry.rs:188-198` (fields); consumer `crates/nif/src/import/mesh/bs_geometry.rs`

Each `BSGeometryMesh` slot carries `tri_size` (triangle-index byte-size hint)
and `num_verts` (vertex-count hint), "always present regardless of
internal/external". The importer never cross-checks these against the
actually-parsed `mesh_data.vertices.len()` / `triangles.len()`. A slot's hint
disagreeing with its resolved `.mesh` body is a strong signal of a wrong-file
resolve (hash collision, stale archive) or a truncated companion — currently
undiagnosable.

Impact: Defense-in-depth gap only; no incorrect render on its own.

Suggested fix: After Stage B parse succeeds, `log::debug!` (or
`debug_assert`) when `data.vertices.len() != num_verts as usize` or the
`tri_size`-derived triangle count disagrees, to surface bad resolves during
bring-up.

Completeness checks called out in the issue:
- SIBLING: same hint-validation gap on the Stage A (internal) parse path
- TESTS: regression test pinning mismatched hint vs. resolved body triggers the debug signal

---

# #1831 — SF3-02: .mat arm silently falls to generic unsupported-format warn when the CDB fails to parse

**Severity**: LOW · **Domain**: binary (`byroredux`)
**Location**: `byroredux/src/asset_provider/material.rs` (`load_starfield_cdb` warn+drop; `.mat` gate; unknown-extension fallback warn)
**Related**: Distinct from #1289 (Phase-2 per-field CDB extraction, tracked separately as SF3-01) — this is about diagnosability of CDB *parse failure*, not the missing value walk.

`load_starfield_cdb` warns and drops on parse failure, leaving `sf_cdbs`
possibly empty. If the ONLY CDB fails (e.g. a future patch bumps CDB
fileVersion past 4 → `UnsupportedVersion`), `has_starfield_cdb()` returns
false, the `.mat` gate is skipped, and every `.mat` mesh falls through to the
unknown-extension arm — logging "unsupported format (Starfield .mat?)" per
path. The operator sees generic per-material spam that does not point back at
the single upstream CDB failure (logged once, far earlier).

Evidence: `load_starfield_cdb` failure branch logs once and returns
(`material.rs:245-256`); the downstream `.mat` warning at line 1081 never
references CDB state. The two log sites are disjoint.

Impact: Diagnosability only. Content still renders (NIF-default Lambert).

Suggested fix: In the `.mat` fallback, when the path ends `.mat` AND
`!has_starfield_cdb()`, emit a distinct once-only warning naming the likely
cause ("Starfield .mat encountered but no CDB loaded/parsed — check
--materials-ba2 and CDB version").

Completeness checks called out in the issue:
- SIBLING: same once-only-warning pattern applied consistently across other CDB/BGSM-gated fallback arms
- TESTS: regression test pinning CDB parse failure produces the distinct warning, not per-mesh spam
