# Issue Batch: 2362, 2364, 2541, 2571

## #2362 — SF2D2-05 (binary, byroredux)
`byroredux/src/cell_loader/object_lod.rs:262`, `placement_lod.rs:469`,
`terrain_lod_btr.rs:137` call the no-resolver `import_nif_scene` overload even
though a `tex_provider: &TextureProvider` (a `MeshResolver`) is already in scope
at each call site. `byroredux/examples/dump_nif.rs:151` calls `import_nif`
(same no-resolver gap) in a debug tool. Currently unreachable for Starfield
(object_lod is `.bto`-keyed, Starfield's LODMeshes.ba2 has none; placement_lod
is Oblivion-gated) but would silently drop external-geometry BSGeometry
meshes to zero once Starfield distant-object LOD is wired.
Fix: thread `Some(tex_provider)` through at the 3 production call sites via
`import_nif_scene_with_resolver`; note or fix the example gap.

## #2364 — SF-D5-2026-08-03-01 (esm, byroredux-plugin)
`crates/plugin/src/esm/cell/walkers.rs:172-174` — the `assert_eq!` failure
message in `starfield_xcll_sizes_pinned` still reads "Skyrim+ 92-byte body +
16-byte SF tail" (the #1291 framing), while the corrected doc comment at
line 39 (from #1293) says XCLL "shares only bytes 0-39 with Skyrim, then
diverges into a distinct volumetric height-fog model." Pure string-literal
fix, no behavior change.

## #2541 — SCR-D7-NEW10-01 (binary, byroredux)
`byroredux/src/cell_loader/references/mod.rs` — the `is_primary_synth` gate
on `stamp_quest_reference`/`spawn_logical_quest_reference` (8 call sites
inside `spawn_synth_child`, correct by inspection) has no regression test.
Suggested fix offers two options: a full spawn-fixture test asserting exactly
one `SceneAliasCandidate` registers for a multi-child SCOL/PKIN expansion, or
a cheaper source-scan test (mirroring `scol_expansion_is_cached_across_a_budget_yield`)
asserting every call site is guarded. Prefer whichever is proportionate once
the surrounding code is read.

## #2571 — OBL-D5-01 (nif/nifal — spans byroredux + likely crates/nif or core)
`texture_clamp_mode`, `src_blend_mode`, `dst_blend_mode` have no canonical
`Material` field — read directly off raw `ImportedMaterial` at 4 spawn call
sites (`spawn.rs:1367,1533,1565`, `nif_loader.rs:786,830,915`) instead of
through `translate_material`. Latent (byte-identical today) but a third
independent reader (FO4 `cell_loader/precombined.rs`) already reads the same
raw fields and could diverge; invisible to `mat.*`/`material_dump` since
those inspect canonical `Material`. Fix: add 3 fields to canonical `Material`,
copy in `translate_material`, point all 4(+1?) spawn sites at the canonical
component, extend the canonical-completeness harness. Needs investigation to
find `Material`'s definition and `translate_material`'s location before
scoping file count.

## Domain classification
- #2362, #2541 → **binary** → `byroredux`
- #2364 → **esm** → `byroredux-plugin`
- #2571 → **nif/nifal**, likely spans `crates/nif` (or wherever canonical
  `Material` lives) + `byroredux` (spawn.rs, nif_loader.rs, precombined.rs) —
  investigate before committing to a test target.

## Plan
Investigate #2571 first since it may hit the >5-file scope-check threshold
before deciding whether to pause for confirmation. The other three are small
and independent.
