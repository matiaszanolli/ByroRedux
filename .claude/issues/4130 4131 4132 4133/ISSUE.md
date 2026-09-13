# Batch: 4130, 4131, 4132, 4133

## #4130 — REG-02: `cargo clippy --workspace --all-targets -- -D warnings` red on `field_reassign_with_default`
- Severity: LOW · tech-debt/CI-gate
- Location: `crates/core/src/ecs/resources/ownership_tests.rs:307,336,386,445` (field_reassign_with_default),
  `crates/core/src/stealth.rs:797` (doc_lazy_continuation)
- Fix: struct-update syntax in the 4 `ownership_tests.rs` sites; fix doc-list indentation in `stealth.rs:797`.
- Domain: ecs → byroredux-core

## #4131 — REG-03: FormID-remap field-level regression test gap
- Severity: LOW · esm-plugin/test-gap
- Location: `crates/plugin/src/esm/records/weather.rs` (MNAM/NNAM), `crates/plugin/src/esm/records/misc/magic.rs`
  (parse_perk EPFD FormId arm, parse_mgef associated_item/effect_shader_id, MagicEffectAccumulator::feed EFID),
  `crates/plugin/src/esm/records/misc/dialogue.rs` (parse_mesg QNAM)
- Fix: add one non-identity-remap test per field (7 fields total), following
  `parse_clmt_wlst_remaps_to_global_form_id_space` / `parse_scpt_remaps_scro_but_never_scrv` pattern.
- Domain: esm → byroredux-plugin

## #4132 — PHYS-D1: Dead `CollisionShape::scaled()` doc comment contradicts corrected scale contract
- Severity: MEDIUM · physics/shape-translation
- Location: `crates/core/src/ecs/components/collision.rs:51-113`
- No live callers (only self-recursive Compound + unrelated RadiantIntensity::scaled). Doc says "any site
  ... must pass it through here first" — contradicts the now-canonical convert.rs contract (producers keep
  local units, converter applies scale exactly once).
- Fix: delete `scaled()` + its tests (no caller), OR rewrite doc to state it must never be combined with
  `collision_shape_to_parts`/`ragdoll::build_ragdoll`.
- Domain: ecs → byroredux-core (component lives in crates/core)

## #4133 — D5-06: TPLT resolution hoist never happened (6bcd1666 patched 4 sites individually)
- Severity: MEDIUM · character/CHARAL
- Location: `byroredux/src/npc_spawn/resumable.rs` (`spawn_placement_root` + 3 other call sites),
  `byroredux/src/npc_spawn.rs`, `byroredux/src/npc_spawn/ai_package.rs`, `byroredux/src/cell_loader/references/mod.rs`
- Investigation found the real scope larger than the issue's own count: 11 independent call sites
  across two crates (the issue's 9 + 2 more inside `byroredux-plugin`'s own `derive_npc_actor_values`
  that it re-resolves independently). Doing the full hoist correctly means threading 4 separately-flagged
  resolved records across a crate boundary — asked the user via AskUserQuestion; chose "add regression
  test only" over the full/partial hoist given zero live impact (latent risk only).
- Fix: added `resolve_inherited_call_sites_are_enumerated_and_pinned` source-scan test
  (`byroredux/src/npc_spawn/tests.rs`) pinning the current 11-site count so a future 12th site can't be
  added silently. The hoist itself remains undone — left for a dedicated follow-up if ever prioritized.
- Domain: character → byroredux (binary)
