# #4964: REN-D6-2026-09-27-02: `every_exterior_spawner_inserts_a_boundary_material`'s doc still says `scene.rs` is out of scope and omits the save / ground-cover exemptions

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4964
- **Labels**: low,nifal,documentation,doc-rot

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D6-2026-09-27-02**._

- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/material_translate.rs:2431-2436` (test doc "Deliberately out of scope: `cornell.rs` and `scene.rs`"), `:2578-2591` (`SPAWNER_ROOTS` doc "two debug-scene files a crate-wide walk would pull in").
- **Status**: NEW
- **Description**:
  - Since `2b1b7fc5c` (#4856), `scene.rs` is itself a `SPAWNER_ROOTS` entry, scanned with a per-entity exemption for `cube`/`quad`/`red_tri`/`blue_tri`. The doc still describes it as excluded and implies only a crate-wide walk would reach it.
  - The guard doc names no exemption for save `restore_world` or EXAL ground cover. Those are recorded in `docs/engine/nifal.md` §3 (lines ~725-731) and `docs/engine/exal-groundcover.md` (the "Material-boundary exemption (#4304)" paragraph). Ground cover's spec does record the exemption.
- **Evidence**: `SPAWNER_ROOTS: [&str; 6] = ["cell_loader","cell_loader.rs","scene","scene.rs","npc_spawn","npc_spawn.rs"]`; the `name == "scene.rs" && [...].contains(&entity)` exemption sits at `:2510`.
- **Impact**: An auditor or a future edit trusting the doc will misjudge what the guard covers.
- **Related**: #4856, #4302, #4304, Existing #4917 (stripper).
- **Suggested Fix**: Rewrite the scope paragraph: cornell.rs is excluded by root choice; scene.rs is scanned with four named exemptions; point to nifal.md §3 for the save and ground-cover exemptions.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
