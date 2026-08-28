# #3536 — LC-2026-08-27-D5-02: assemble_exterior_streaming carries an undocumented game == Skyrim branch with two hardcoded vanilla FormIDs

Labels: low, bug, legacy-compat, terrain-exterior, tech-debt, game:skyrim
Source: docs/audits/AUDIT_LEGACY_COMPAT_2026-08-27.md (base 969d81c8)
Filed: 2026-08-27 via /audit-publish

---

**From:** `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-27.md` (LC-2026-08-27-D5-02) · base `969d81c8`

- **Severity**: LOW
- **Dimension**: 5 — EXAL boundary shape ("scattered new `if game == …` exterior logic is a finding")
- **Location**: `byroredux/src/scene/world_setup.rs:933-941`

## Description

The shared exterior-streaming assembly ends with:

```rust
if state.wctx.record_index.game == byroredux_plugin::esm::reader::GameKind::Skyrim {
    crate::asset_provider::materialize_scene_actor_alias_stubs(
        world,
        &state.wctx.record_index,
        &state.wctx.load_order,
        0x0003_372B,
        0x000B_ECD4,
    );
}
```

with **no comment at the call site**. `materialize_scene_actor_alias_stubs` (`byroredux/src/asset_provider/script.rs:577`) is itself a properly general, well-documented helper parameterised on quest + scene FormID, and this is its **only** caller. So the whole per-title specificity — the `GameKind` gate and the two literal Skyrim MQ101 form IDs — sits in `assemble_exterior_streaming`, which is the common entry point for four callers (boot's `--grid` mode, `App::step_cell_transition`'s Exterior arm, the `dbgload` exterior command, and `save_io`'s reload path — enumerated in `begin_exterior_streaming`'s own doc at `:947-963`).

## Evidence

The snippet above, verbatim from HEAD (`world_setup.rs:933-941`); `grep -rn materialize_scene_actor_alias_stubs byroredux/src` returns exactly the definition (`asset_provider/script.rs:577`) and this one call.

## Impact

No runtime impact today — it is gated, and the M47.2 MQ101 slice is a deliberately scoped demo. The cost is shape: this is the seed of the per-title-content-hack pattern in a game-agnostic path. A second scoped scene (any game) has nowhere to go but a second arm here, and there is no in-code pointer to `docs/engine/m47-2-design.md` telling a reader that the scope is intentional. It also silently means no non-Skyrim title gets forced quest-alias stubs even where its content needs them. Related but distinct from **#2664 (CLOSED)**, which fixed this same code's *open-coded stamper* and left the gate untouched.

## Related

#2664 (closed), `docs/engine/m47-2-design.md`. The correct in-tree shape to copy: `terrain_lod_layout` (`byroredux/src/env_translate.rs:61-64`) and `object_lod_scheme` (`byroredux/src/cell_loader/object_lod.rs:458`).

## Suggested Fix

Either move the (game, quest, scene) triple into a small table beside the other per-game exterior decisions (the `terrain_lod_layout` / `ObjectLodScheme` shape), or leave it in place with a three-line comment naming MQ101, pointing at `m47-2-design.md`, and stating the intended scope — so the next auditor does not have to reverse-engineer two hex literals to decide whether it is a leak.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other per-game exterior decision points — `terrain_lod_layout`, `object_lod_scheme`, `lod_support.rs`, the Oblivion water default at `env_translate.rs:187`)
- [ ] **TESTS**: A regression test pins this specific fix (if the triple moves to a table, a table test in the shape of #3321's scheme/band-ladder pin)
