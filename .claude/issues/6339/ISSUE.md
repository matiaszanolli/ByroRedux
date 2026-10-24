# PERF-D1-08: find_package_system scans the whole candidate set per target-less actor each second

- **Issue**: #6339 (snapshot as filed; GitHub is authoritative for live state)
- **Labels**: low, performance, gameplay, ai, bug
- **Source**: docs/audits/AUDIT_PERFORMANCE_2026-10-24.md

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/find_package.rs:115-170` (`search`; the `item_kind` lookup at `:77-85,144`), `:174-182` (`behaviors` Vec), `:213-227` (search clock), `:230-245` (`is_actor` / `actor_would_attack` / `is_pickup_target` per frame), `:96-103,176-180` (scratch maps).
- **Description**: three separable costs.
  1. *The search.* For a Base or Kind target the actor walks **every** `SceneAliasCandidate` row once per second until it finds something. A Kind search evaluates `item_kind(index, base)` per row, i.e. `index.items.get(&base)` on a std (SipHash) `HashMap<u32, ItemRecord>` (`crates/plugin/src/esm/records/index.rs:25,280`), and the radius test comes after the kind filter. Actors installed in the same frame keep aligned clocks (`entry starts at 0`), so K such actors search on the same tick.
  2. *Per-frame allocation and re-derivation.* `behaviors` is a fresh `Vec` every frame any Find actor exists (sibling `flee_package.rs` keeps persistent scratch, #2033). For a held actor target `actor_would_attack` runs every frame: it clones `FactionRelations`, takes an `Arc` of the catalog and builds two `Vec<FormRef>` per call (`faction_hostility.rs:202-228`). `is_pickup_target` makes four sparse lookups plus a catalog probe per frame for an item target. An actor that has arrived at a friendly NPC (`Arrival::Stand`, never completes) also re-writes `FindProgress` through a `query_mut` every frame.
  3. *Scratch growth.* `search_clock` and `stuck` are cleared only when **no** actor carries `FindPackageBehavior`; entries for actors that streamed out or changed package are never pruned (unlike `EatSleepScratch`, #5707). 12–16 bytes per Find actor ever seen per session.
- **Evidence**: measured shape probe (CPU table). The per-frame items in point 2 are derived: about 0.3–1 µs per held actor-frame.
- **Impact**: derived. A Kind-searching actor with no target costs a 0.2 ms spike (20 k refs) to 0.6 ms (60 k refs) once a second, multiplied by the number of aligned searchers; about 3–10 µs per searcher per frame averaged at 60 fps. No sustained cost beyond that.
- **Related**: PERF-D1-07; `interaction.rs:1259-1325` already notes that item placements are a small share of the `SceneAliasCandidate` set.
- **Suggested Fix**: precompute `base -> Option<FindItemKind>` once per `LoadedCellIndex` swap (an `FxHashMap`, or fold into `InventoryCatalog`, which `is_pickup_base` already consults) and do the cheap `GlobalTransform` radius test before the kind classification. Stagger each actor's first `search_clock` (for example `entity % period`). Make `behaviors` persistent scratch, evaluate the arrival class only when `progress.target` changes, and prune `search_clock`/`stuck` against the live behavior set as `EatSleepScratch.live` does.


## Completeness Checks
- [ ] **SIBLING**: the same per-frame pattern is checked in the neighbouring package/AI systems (Follow, Escort, Travel, Guard already resolve once, #5724)
- [ ] **LOCK_ORDER**: if a query/resource lock scope changes, TypeId-sorted acquisition is preserved (the flee/find systems were already reworked once for an ABBA cycle, `ce22fcbcf`)
- [ ] **TESTS**: a regression test pins the fix (a resolve-count / scratch-capacity / settled-pass test in the style of `a_settled_pass_skips_the_walk_until_an_actor_appears`)

_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-10-24.md`. Costs are labelled measured/derived in the report; no GPU frame time is claimed._
