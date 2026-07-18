# Batch: TD1-006 / TD1-007 / TD1-008 / TD1-009 — Function/File complexity

All LOW severity, Dimension 1 (File/Function/Module Complexity). Pure mechanical splits.

## #2055 TD1-006 — actor.rs / parse_npc
`crates/plugin/src/esm/records/actor.rs` (2288 LOC). `parse_npc` (~line 505) is a 332-line, 29-arm
sub-record match interleaving identity/faction, inventory, runtime FaceGen, FO4 pre-baked FaceGen,
actor-value props. Extract per-group helpers; extract 960-line test module.
Precedent bug: #1996 (divergent branch). Crate: byroredux-plugin.

## #2056 TD1-007 — shader_tests.rs
`crates/nif/src/blocks/shader_tests.rs` (2101 LOC). Test module past 2000 LOC. Split along era
boundaries (legacy/Skyrim/FO4/FO76/Starfield). Deferrable, mechanical. Crate: byroredux-nif.

## #2057 TD1-008 — cell_loader/spawn.rs / spawn_placed_instances
`byroredux/src/cell_loader/spawn.rs` (1316 LOC). `spawn_placed_instances` (~line 180) is a
1065-line function (81% of file). Split placement-root setup vs per-mesh helper. Crate: byroredux.

## #2058 TD1-009 — cell_loader/references/mod.rs / load_references
`byroredux/src/cell_loader/references/mod.rs` (1560 LOC). `load_references` (~line 92) is a
1015-line function (69% of file). Continue #1877 split one level deeper — per-record-kind dispatch.
Crate: byroredux.
