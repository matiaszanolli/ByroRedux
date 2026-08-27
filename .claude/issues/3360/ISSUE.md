# SKY-2026-08-27-D1-02: `triangle_body_parts` applies the same inverted `vertex_map` remap — currently self-cancelling, and it silently breaks SSE dismember/equip hiding the moment D1-01 is fixed

Labels: medium,nif-parser,nif,bug,game:skyrim,legacy-compat

- **Severity**: MEDIUM
- **Confidence**: CONFIRMED (code-read; same wire semantics proven in D1-01)
- **Location**: `crates/nif/src/import/mesh/skin.rs:35-44` (the `vertex_map` remap inside `triangle_body_parts`, `skin.rs:16-67`)
- **Description**:
  `triangle_body_parts` builds a `canonical_triangle -> body_part` map by walking the same
  `NiSkinPartition.partitions[i].triangles` and pushing each index through `part.vertex_map`:

  ```rust
  for (dst, &local) in global.iter_mut().zip(triangle) {
      if part.vertex_map.is_empty() {
          *dst = local as u32;
      } else if let Some(&mapped) = part.vertex_map.get(local as usize) {
          *dst = mapped as u32;
      } else { valid = false; break; }
  }
  ```

  For SSE partitions this is the same inverted interpretation proven in D1-01. Today it is
  *masked*: the `final_indices` it looks its keys up against are produced by
  `try_reconstruct_sse_geometry`, which applies the identical wrong remap and skips the
  identical set of triangles, so the two sides agree and body parts land on the (mis-indexed)
  surviving triangles. The moment D1-01 is fixed in `sse_recon.rs` alone, `final_indices`
  becomes global while these keys stay remapped, no key matches, every entry falls to
  `UNASSIGNED_BODY_PART`, and the function's own trailing guard
  (`if mapped.iter().any(|&part| part != UNASSIGNED_BODY_PART)`) returns `Vec::new()`.

  The remap is still **correct** for the legacy path: `bMappedIndices` defaults to `true` and
  only flips to `false` for `Stream() == 100` (`nifly Skin.cpp:82-85`), so Oblivion/FO3/FNV
  `NiSkinPartition` triangles genuinely are vertex_map-local, and `extract_skin_ni_tri_shape`
  (`import/mesh/skin.rs:135`) routes those through the same function. The fix therefore has to
  be a version gate, not a deletion.

- **Evidence**: the code above at `skin.rs:35-44`; its two call sites at `skin.rs:135`
  (`extract_skin_ni_tri_shape`, legacy) and `skin.rs:162` (`extract_skin_bs_tri_shape`, SSE+);
  the consumer `ImportedMesh::hide_skin_partitions` at `crates/nif/src/import/types.rs:1118-1135`,
  which is a no-op unless `skin.triangle_body_parts.len() == old_triangle_count`. The SSE wire
  semantics are the same ones proven in D1-01 (nifly `Skin.cpp:82-85`, `Skin.hpp:105-109`, and
  the 56,259,423/56,259,423 subset measurement).

- **Impact**: latent today. On fixing D1-01 in isolation, `triangle_body_parts` returns empty
  for every Skyrim SE skinned mesh, so `hide_skin_partitions` stops hiding anything and the M41
  outfit-equip path renders bare body skin through every piece of armour on every NPC —
  a regression that the parse-rate gate and `cargo test -p byroredux-nif` would both pass.

- **Suggested Fix**: gate on the partition itself rather than on the game: use the raw
  `part.triangles` as global indices when `partition.global_vertex_data.is_some()` (the exact
  SSE marker `NiSkinPartition::parse` already computes), and keep the existing `vertex_map`
  remap for the `None` case (Oblivion/FO3/FNV). Land it in the same commit as D1-01 so neither
  half is ever live alone.

- **Related**: same root cause as SKY-2026-08-27-D1-01. Nothing in the 300-issue dedup baseline (84 open, fetched 2026-08-27)
  covers it. Adjacent but distinct from #3187 (slot swap).

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27.md` (`/audit-skyrim`, 7 dimensions),
verified against HEAD `558af58c` on a full vanilla Skyrim SE install.*
