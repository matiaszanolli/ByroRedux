# EXT-D1-2026-10-26-02: #6089 widened `apply_exterior_processing` to FO3/FNV but left the shared-parse `exterior_ready` predicate Oblivion-only, so an FO3/FNV interior-first parse is served to the ex...

Labels: medium,terrain-exterior,esm-plugin,bug,game:fo3,game:fnv
Source: docs/audits/AUDIT_EXTERIOR_2026-10-26.md

- **Severity**: MEDIUM
- **Dimension**: 1 EXAL boundary discipline (parse-boundary handoff)
- **Tier Violated**: single-boundary (two predicates, `apply_exterior_processing`'s game gate and `get_or_parse`'s `exterior_ready`, now disagree about which games need exterior processing)
- **Game Affected**: FO3 and FNV (`GameKind::Fallout3NV`): Oasis, ParadiseFalls, GNRroofWorld, MegatonWorld and any "Use Land Data" child
- **Location**: `byroredux/src/cell_loader/load_order.rs:614-626` (`apply_exterior_processing`, now `Oblivion | Fallout3NV`) vs `load_order.rs:702` (`let exterior_ready = exterior || index.game != esm::reader::GameKind::Oblivion;`); doc at `load_order.rs:683-686`
- **Status**: Regression of #6123 (`588c87bc4`) caused by a9472103b (#6089). Static trace; unconfirmed live (needs FO3 data).
- **Description**: #6123 made `SharedParseCache` serve one parse to both interior and exterior sessions, and marked an index `exterior_ready` when parsed for an exterior *or* when its game needs no exterior processing (then only Oblivion did). #6089 added `Fallout3NV` to `apply_exterior_processing` but did not touch line 702. A session that boots into an FO3/FNV interior (`--cell`, or a load into an interior) parses with `exterior == false`; `inherit_parent_landscapes` is not run, yet `exterior_ready` is computed as `true` for `Fallout3NV`. The later exterior session (door to the Wasteland, `build_exterior_world_context`) calls `get_or_parse(.., exterior = true)`, finds `shared.exterior_ready`, and reuses the unprocessed index. The "Use Land Data" children then have no ground again, which is exactly the FO3-SD-03 symptom #6089 closed, but only for interior-first sessions.
- **Evidence**: `a_second_transition_over_the_same_plugins_does_not_parse` parses `GameKind::Fallout3NV` with `exterior=false` first, then asserts `Arc::ptr_eq` for `exterior=true` requests, i.e. it encodes the now-wrong behaviour; only the Oblivion test (`an_oblivion_interior_parse_is_not_served_to_an_exterior_session`) covers the re-parse. The #6089 census pin (`fo3_use_land_data_children_inherit_the_wasteland_landscape`) is `#[ignore]` and calls `inherit_parent_landscapes` directly, so nothing in the default suite pins the gate or the predicate.
- **Impact**: FO3/FNV Use-Land-Data towns (Oasis 34, ParadiseFalls 11, GNRroofWorld 58, MegatonWorld 5 LAND-less cells) draw no terrain when the first cell loaded was an interior. Workaround: boot with `--grid`/exterior first.
- **Related**: #6123, #6089 (closed), #5505, FO3-SD-03.
- **Suggested Fix**: one `fn needs_exterior_processing(game: GameKind) -> bool` (table-shaped) used by both `apply_exterior_processing` and the `exterior_ready` expression; add an FO3 case to the cache test (an `exterior=false` parse of `Fallout3NV` must be re-parsed by an `exterior=true` request) and flip the existing FO3 assertion in `a_second_transition_over_the_same_plugins_does_not_parse` to a non-processed game.

_Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-26.md` (HEAD 3e6f6b66a). Static trace; not confirmed on screen unless stated._

## Completeness Checks
- [ ] **EXAL-BOUNDARY**: The fix keeps environment translation at the single `translate_*` / `install_environment` boundary — no per-game logic pushed to the renderer or re-derived at render time. See `/audit-exterior`.
- [ ] **SIBLING**: Same pattern checked in related files (other shader copies, other sun/lighting sites, other rings)
- [ ] **TESTS**: A regression test pins this specific fix
