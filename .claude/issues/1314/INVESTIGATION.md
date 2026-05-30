# #1314 (OBL-D6-NEW-04) — ALREADY FIXED (duplicate of closed #1308)

No code change. The Oblivion mod-index-01 FormID artifact is already handled; #1314
is a re-file of the finding resolved by #1308.

Current `crates/plugin/src/esm/reader.rs::FormIdRemap::remap` (`:265-285`): on a
standalone / single-plugin load (`master_indices.is_empty()`) with a top byte past
index 0, the form is passed through unchanged and logged at **debug** (the prior
per-form **warn** spam — and the false "single-plugin never reaches here" comment —
were the #1308 fix). It deliberately does NOT clamp to self: whether a 0x01 local-id
aliases a real 0x00 form is unverified, and a wrong clamp would collide two distinct
forms onto one global FormID (strictly worse than the harmless dangling pass-through;
no rendering impact). Cites `#1308 / OBL-D6-NEW-04`.

Test: `form_id_remap_standalone_out_of_range_passes_through` (`:720`) pins
`remap(0x0100_0ABC) == 0x0100_0ABC` etc. SIBLING (FO3/FNV): the standalone branch is
game-agnostic, and `form_id_remap_two_dlcs_resolve_collision` (`:756`) covers the
FO3 Anchorage/BrokenSteel 0x01 collision. Closed as duplicate of #1308.
