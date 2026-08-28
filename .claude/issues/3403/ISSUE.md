# #3403 — ESM-2026-08-27-D7-01: merge_from last-write-wins on EsmIndex::game — one failed plugin parse silently re-labels the whole load order as Fallout 3/NV

**Labels**: medium, esm-plugin, bug
**Source**: `docs/audits/AUDIT_ESM_2026-08-27.md`

---

**Audit**: `docs/audits/AUDIT_ESM_2026-08-27.md` (`/audit-esm`, deep, tree `main` @ `969d81c8`)
**Severity**: MEDIUM · **Dimension**: `EsmIndex` → ECS Handoff
**Record / Sub-record**: `TES4` / `HEDR`
**Location**: `crates/plugin/src/esm/records/index.rs` (`merge_from`, the `self.game = other.game` line); reachable via `byroredux/src/cell_loader/load_order.rs` (`parse_record_indexes_in_load_order`)
**Status as filed**: NEW — and explicitly deferred to this audit by the commit that fixed the sibling case

## Description

`#3384` (`aec3e1ca`) fixed exactly this hazard for `character_rules`: `merge_from` used to do an unconditional last-write-wins, and the load-order driver merges an `EsmIndex::default()` whenever a plugin fails to parse, so a failure in the *last* plugin adopted the default and switched the whole character layer off. The fix guarded `character_rules` with first-non-`NONE`-wins — and left the identically-shaped `game` field one line above it unguarded, with a comment saying so:

```rust
// NOTE: this field is deliberately *not* given the `character_rules`
// guard below. Callers construct an index and set `game` without ever
// setting `character_rules` (the FO4 inventory-classification path
// does exactly that), so gating the two together would silently drop
// the game. The same empty-index hazard applies to `game` in
// principle; it is out of scope here and belongs to /audit-esm.
self.game = other.game;
```

The reasoning for not reusing the *same* guard is sound; the hazard it names is real and is now filed here.

## Evidence

`byroredux/src/cell_loader/load_order.rs`:

```rust
let plugin_records = esm::records::parse_esm_with_load_order(&bytes, Some(remap))
    .unwrap_or_else(|e| {
        log::warn!("Record parse failed for '{}': {}", path, e);
        esm::records::EsmIndex::default()
    });
merged.merge_from(plugin_records);
```

`EsmIndex::default()` carries `game: GameKind::default()` = `GameKind::Fallout3NV` (`crates/plugin/src/esm/reader.rs`), so the last plugin's parse failure overwrites a correctly detected `Skyrim` / `Fallout4` / `Starfield` with `Fallout3NV` behind a single `warn`.

## Impact

`EsmIndex::game` is a broad dispatch key, not a label. Live consumers include `byroredux/src/npc_spawn.rs` (`humanoid_skeleton_path`, `humanoid_body_path_biped_mask`, `humanoid_default_idle_kf_path`, `sandbox_sit_enter_kf_path`), `byroredux/src/asset_provider/script.rs` (Papyrus `.pex` translation), `byroredux/src/asset_provider/animation.rs` (the Havok slice's Skyrim gate), `byroredux/src/env_translate.rs` (`terrain_lod_layout`), `byroredux/src/cell_loader/placement_lod.rs`, `byroredux/src/inventory.rs` (`player_npc_form_id`) and `byroredux/src/streaming.rs`. A misclassified load order therefore spawns actors with the wrong skeleton, resolves the wrong player base form, and picks the wrong terrain-LOD layout — all downstream of a warning about an unrelated subject.

## Related

`#3384` (the `character_rules` half, fixed); `#2907` (the earlier `merge_from` completeness fix).

## Suggested Fix

Do not reuse the `character_rules` guard (the comment explains why). Instead make the failure site honest: either propagate the parse error out of `parse_record_indexes_in_load_order` rather than merging a default index, or have `merge_from` skip `game` when `other` is observably empty (`other.total() == 0`). The second is a one-line change and matches the "a failed parse contributes nothing" intent the `warn!` already implies.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (every other scalar field `merge_from` last-write-wins on)
- [ ] **TESTS**: A regression test pins this specific fix
