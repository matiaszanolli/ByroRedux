# Issues 2613, 2614, 2615, 2616 — HIGH severity: Starfield skinning + CDB safety + shader alignment

## #2613 — SF2D2-D2-01 (HIGH): Starfield skinned meshes render in bind pose
- Files: crates/nif/src/import/mesh/skin.rs (extract_skin_bs_geometry, canonical fix site), crates/nif/src/import/mesh/bs_geometry.rs:249-260 (call site), crates/nif/src/blocks/bs_geometry.rs (BSGeometryMeshData.skin_weights, already decoded)
- extract_skin_bs_geometry hardcodes vertex_bone_indices/vertex_bone_weights to Vec::new() with a stale "not decoded yet" comment; skin_weights IS decoded (BoneWeight{bone_index:u16, weight:u16}), just never plumbed through.
- Fix: pass mesh_data into extract_skin_bs_geometry; when weights_per_vert>0 && !skin_weights.is_empty(), map to [u16;4]/[f32;4] (top-4-by-weight, zero-pad, /65535.0, renormalize via crates/nif/src/blocks/tri_shape/mod.rs::renormalize_skin_weights, same as FO4 BsTriShape). Guard skin_weights.len()==vertices.len(), fallback bind-pose on mismatch. Update stale test at bs_geometry_skin_tests.rs:118-121. Correct #1827's stale closed-issue premise.

## #2614 — SF-D3-01 (HIGH): index_chunks pre-reserves from unvalidated on-disk u32, aborts on corrupt CDB
- File: crates/sfmaterial/src/reader.rs:172-179 (index_chunks)
- VecDeque::with_capacity(chunk_count) sized directly from unvalidated on-disk u32 (up to ~4B, ~103GB for u32::MAX) BEFORE the ChunkOverflow guard runs -> panic/abort, not catchable.
- Fix: chunks.reserve(chunk_count.min(self.bytes.len() / 8)) or drop with_capacity entirely (each chunk costs >=8 bytes). Add fuzzed chunk_count (0xFFFFFFFF) test asserting Err not panic.

## #2615 — SF-D3-03 (HIGH): Archive::open reads entire multi-GB archive to sample 4 magic bytes
- Files: byroredux/src/asset_provider/archive.rs:10-27 (Archive::open), byroredux/src/asset_provider/material.rs:194-205 (build_material_provider, re-opens for file table)
- std::fs::read(path) allocates+fills a Vec<u8> the size of the WHOLE archive just for magic-byte dispatch; each mesh archive fully read into RAM twice per provider build (~6 call sites).
- Fix: File::open(path)?.read_exact(&mut [0u8;4]) instead of fs::read. Add test asserting Archive::open reads only a small fixed byte count via a byte-counting reader wrapper.

## #2616 — SF-D6-01 (HIGH): BSLightingShaderProperty misaligned by one 4-byte word on Starfield full-body blocks
- File: crates/nif/src/blocks/shader.rs:1142-1161 (BSLightingShaderProperty::parse_fo76_plus)
- Two compensating 4-byte errors for bsver >= STARFIELD: skips shader_type (Starfield DOES carry it) and reads root_material_path unconditionally (Starfield does NOT carry it) -> every field between them (num_sf1/num_sf2, CRC arrays, uv_offset/uv_scale, texture_set_ref, emissive_color, emissive_multiple) reads one word early. Corpus-verified: 0/2538 valid under shipped alignment, 2538/2538 valid under corrected.
- Fix: read shader_type unconditionally for bsver>=FO76 (revert the <STARFIELD gate); gate root_material_path on bsver<STARFIELD instead. Net byte count unchanged. Add real-data-derived fixture (shiplandingmarker_lod_3.nif block 6) asserting semantic invariants (finite emissive, resolvable texture_set_ref, valid CRC membership).
