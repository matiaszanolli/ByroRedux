# FO4-2026-10-31-01: FO4 placed-object behavior graphs never attach: the character's `rig` path is resolved verbatim

Labels: medium,bug,animation,import-pipeline,legacy-compat,game:fo4

Filed from `docs/audits/AUDIT_FO4_2026-10-31.md` (finding `FO4-2026-10-31-01`).

- **Severity**: MEDIUM. It is a whole content class: about 330 animated-prop base records and about 6,200 REFRs in Fallout4.esm alone.
- **Dimension**: 4, FO4 data through the shared M47.2 object-graph mechanism. The runtime belongs to `/audit-gameplay` Dim 5.
- **Location**:
  - `byroredux/src/asset_provider/object_graph.rs:176`: `load_graph`, where `fetch(&relative(&character.rig), "skeleton")?` returns `None` on a miss.
  - `object_graph.rs:159`: the `relative` closure joins `directory` and the name with no `..` collapse and no extension mapping.
  - `byroredux/src/asset_provider/archive.rs:593-615`: `normalize_mesh_path`.
  - `crates/bsa/src/ba2.rs:1265`: `normalize_path` is only a lowercase and a slash swap.
- **Status**: NEW. No open or closed issue found (`gh` search, plus the open snapshot).
- **Description**:
  - `load_graph` decodes project → character → behavior → skeleton for a placed NIF's `BSBehaviorGraphExtraData`. A missing skeleton returns `None`, so the entity is marked stuck and gets no `ObjectAnimationGraph`.
  - FO4's characters name the rig the way the Havok toolchain wrote it. That is a `.hkt` extension, and relative `..\` hops to a shared single-bone skeleton: `..\..\GenericBehaviors\zSingleBoneSkeleton\SingleBoneSkeleton.hkt`, or `CharacterAssets\Skeleton.HKT` beside the project.
  - The archives ship `.hkx` only (`meshes\genericbehaviors\zsingleboneskeleton\singleboneskeleton.hkx`), and the BA2 lookup is an exact hash-map hit.
  - The actor route avoids this by hardcoding `characterassets\skeleton.hkx` (`actor_graph.rs:120`, `archetype_idle.rs:146`) and mapping clip names by stem, so FO4 actors work and FO4 props do not. Skyrim's `.hkx` spellings resolve, which is probably how the prop route passed its tests.
- **Evidence** (probe `/mnt/data/tmp/fo4probe/src/bin/graphs.rs`, replicating `load_graph`'s chain over `Fallout4 - Animations.ba2` + the 8 mesh archives):
  - 846 non-precombined NIFs carry both a `NiControllerSequence` and a `BSBehaviorGraphExtraData`. They reference 187 distinct projects.
  - **0 of 187 load.** The 187 break down as follows:

    | Failure | Projects |
    |---|---:|
    | skeleton not found: `..\`-relative `.hkt` | 121 |
    | skeleton not found: `CharacterAssets\Skeleton.HKT` | 62 |
    | project file not found | 2 |
    | behavior file not found | 1 |

    - 121 of the skeleton failures would resolve after `..` collapse plus `.hkt` → `.hkx` (`collapsed-hkx-exists=true`).
    - The 62 `CharacterAssets\Skeleton.HKT` cases do not resolve even then. Their skeleton is not at the project's own path.
  - The project and behavior misses are authoring errors in the vanilla data: `GenericBehaviors\Autoplay.hkx` should be `GenericBehaviors\Autoplay\Autoplay.hkx`.
  - Fallout4.esm placed REFRs of bases whose model has a graph and sequences (probe `refs`):

    | Base type | Bases | REFRs |
    |---|---:|---:|
    | ACTI | 187 | 3,359 |
    | FURN | 50 | 715 |
    | MSTT | 44 | 1,039 |
    | STAT | 20 | 682 |
    | LIGH | 18 | 310 |
    | other | — | the remainder |

  - These are raw REFR counts. STAT bases that a precombine bake absorbs do not spawn.
- **Impact**:
  - Autoplay lights (`wallemergencylight01flashomni`), fire gratings, pipe leaks, workshop conveyors and switches, cryo pods and jukeboxes never animate on FO4.
  - Before #6471 the first sequence at least played. For 124 `autoplay`/`autoloop` meshes (each `["autoplay"/c2, "autoloop"/c0]`, no `Idle`), that first sequence was `autoplay`, played once. #6471's `spawn_handle` (`Idle`, else `SpecialIdle`) now plays nothing, and there is no graph to take over.
  - When the graph does attach, `populate_object_graphs` despawns the NIF's embedded player. So fixing the paths also has to keep that handoff.
- **Related**: #6471 (69af7947a; its FO4 leg is -02), M47.2 `scripting.md` §"Object graphs" (claims FO4 actor graphs only), #6126, FNV-2026-10-31-02.
- **Suggested Fix**:
  - Resolve a character's rig the way the actor route does: collapse `..` against the project directory, then try `.hkx` for a `.hkt` spelling.
  - Add an `--ignored` FO4 pin over a couple of vanilla props, for example `DLC05WorkshopConveyorBelts` and `GenericBehaviors\Autoplay\Autoplay.hkx`, asserting that `load_graph` returns `Some`.
  - The 62 `CharacterAssets\Skeleton.HKT` projects need a sourced rule for where their skeleton lives. Do not guess it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games / siblings)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
