# FNV Compatibility Audit — 2026-07-15 (Dimension 9: AI Packages & Sandbox Behavior)

Scope: `--focus 9` — single-dimension run of `/audit-fnv`. Covers M41.5/M42
(NPC AI package selection, CTDA condition gating, sandbox seat assignment).
Verified against the tree as of 2026-07-15.

## Executive Summary

Six of six regression guards for this dimension passed clean — CTDA fail-open,
schedule-priority gating, PLDT radius fallback, the documented NearReference
non-resolution, per-marker seat reservation keying, and the legacy-marker
over-match all match the intended M41.5/M42 design with no drift.

One **NEW HIGH** finding: `parse_npc` is the sole ESM record parser that never
threads a `FormIdRemap` through its embedded FormID fields, including `PKID`
(`ai_packages`) — the exact field this dimension's package-selection logic
consumes. On any FNV load with more than one plugin (base + DLC, base + mod —
including the engine's own documented `--master FalloutNV.esm --esm
DeadMoney.esm` invocation), every non-base-plugin NPC's package lookups
silently miss, and that NPC never sandboxes. No crash, no log — it's an
invisible content-scope regression that only manifests once a real multi-plugin
FNV/DLC load is exercised, which explains why it survived to now (this
engine's own bench-of-record commands are single-plugin).

Per the severity decision tree (silent content-scope failure, no error signal,
affects every multi-plugin load): **HIGH**, consistent with the dimension
checklist's framing of silent-drop bugs.

## Dimension Findings

### AI Packages & Sandbox Behavior

#### DIM9-01: NPC record's own embedded FormID fields (including PKID/ai_packages) are never remapped to global load-order space
- **Severity**: HIGH
- **Location**: `crates/plugin/src/esm/records/actor.rs:493` (`parse_npc` signature, no `remap` param), `:580-584` (`PKID` arm), `crates/plugin/src/esm/records/mod.rs:480-487` (call site, no remap threaded)
- **Status**: NEW
- **Description**: Every other per-record parser that carries embedded FormID references explicitly remaps them from plugin-local to global load-order space via `reader.get_form_id_remap()` — `parse_pack` (PLDT + CTDA, `misc/ai.rs:199-273`), `parse_qust`, `parse_perk`, `parse_avif`, `parse_dial` (QSTI), `parse_info` (TCLT/PNAM/ANAM/CTDA) all take a `remap: &Option<FormIdRemap>` parameter and remap every embedded FormID before storing it. `parse_npc` has no such parameter — its signature is `parse_npc(form_id: u32, subs: &[SubRecord], game: GameKind)` — and the call site never obtains or threads a remap. Every embedded FormID field on `NpcRecord` is stored raw from the sub-record bytes: `RNAM` (race), `CNAM` (class), `VTCK` (voice), `SNAM` (faction), **`PKID` (`ai_packages: Vec<u32>`)**, `DOFT` (outfit), `INAM` (death item), `TPLT` (template), `CNTO` (inventory). `EsmIndex.packages`, by contrast, is keyed by properly-remapped global FormIDs. `npc_spawn.rs`'s `npc.ai_packages.iter().filter_map(|pk| index.packages.get(pk))` thus compares an unremapped local PKID against a remapped global key.
- **Evidence**:
  ```rust
  // actor.rs:493 — no remap parameter at all
  pub fn parse_npc(form_id: u32, subs: &[SubRecord], game: GameKind) -> NpcRecord {
      ...
      b"PKID" if sub.data.len() >= 4 => {
          record.ai_packages.push(SubReader::new(&sub.data).u32_or_default());
          // raw plugin-local value, never passed through remap_fid
      }
  ```
  ```rust
  // mod.rs:486 — call site never fetches/threads a remap, unlike PACK/QUST/PERK/AVIF below it
  b"NPC_" => extract_records_with_modl(&mut reader, end, b"NPC_", &mut statics,
      &mut |fid, subs| { index.npcs.insert(fid, parse_npc(fid, subs, game)); })?,
  ...
  // mod.rs:589-594 — the PACK sibling right below it DOES remap
  let pack_remap = reader.get_form_id_remap();
  ... index.packages.insert(fid, parse_pack(fid, subs, &pack_remap));
  ```
  A single-plugin load (e.g. `--esm FalloutNV.esm` alone) never surfaces this because a standalone master's `FormIdRemap` is identity (`form_id_remap_single_plugin_is_identity` test, `esm/reader.rs:846`). It only manifests once a second plugin enters the load order.
- **Impact**: For any FNV load with more than one plugin, every NPC defined in the non-base plugin has its `PKID` values compared against the wrong (unremapped) top byte, so package lookups miss for all of that plugin's own packages — `active_package_is_sandbox` returns `false` unconditionally and the NPC never gets a `SandboxBehavior` marker regardless of its authored packages. No error, no log, no crash. The same root cause also silently affects `race_form_id`/`class_form_id`/`voice_form_id`/faction/outfit/death-item/template/inventory FormIDs on the same `NpcRecord` — a broader NPC-spawn correctness issue outside this dimension's scope but worth flagging to whoever owns NPC-spawn.
- **Related**: Shares the exact remap mechanism introduced for #1666 ("M47.1 condition: implement GetIsID — needs CTDA form-id remap resolver") and used consistently by `parse_pack`/`parse_qust`/`parse_perk`/`parse_avif`/`parse_dial`/`parse_info` — `parse_npc` is the one holdout never updated to the same pattern.
- **Suggested Fix**: Add a `remap: &Option<FormIdRemap>` parameter to `parse_npc`, thread `reader.get_form_id_remap()` through at the `mod.rs:486` call site (mirroring the PACK/QUST/PERK arms immediately below it), and apply remapping to every embedded FormID field populated in the sub-record loop — `PKID` in particular for this dimension's purposes.

## Regression Guard List (all verified, no drift)

1. **CTDA fail-open (M42.2)** — `package_conditions_pass` (`npc_spawn.rs:1525-1544`) still fails open on any `ConditionFunction::Unknown` in a package's condition list, preserving M42.1 behavior.
2. **Schedule gating** — `active_package` (`ai.rs:152-160`) still picks the first priority-ordered, schedule-active, condition-passing package; unit test covers the "AtBar outranks evening Sandbox" case.
3. **PLDT search radius fallback** — radius-0 and no-PLDT packages both collapse to `None`, falling back to the documented 512-unit `SEAT_SEARCH_RADIUS` default (`sandbox.rs:62,197`).
4. **NearReference center resolution** — still deliberately unresolved, consistent with the 2026-07-14 investigation (1822 packages, ~12% theoretically resolvable). No drift from documented state.
5. **Seat reservation keying** — `SeatReservations: HashSet<(EntityId, u32)>` still keys by `(furniture, marker index)`; unit test proves independent multi-marker reservation on one furniture entity (commit `0a21d5f9`).
6. **Legacy marker over-match** — still honestly documented as a known v0 gap in both module doc and `is_sit_marker` doc comment, not silently patched by a heuristic.

Also checked and clean: `PackSchedule::active_at` midnight-wrap handling, `condition::evaluate`'s OR/AND block-walker OOB guard (#1767), `SeatReservations` has no release path (matches documented v0 scope, not a bug), `BYRO_SANDBOX_SIT` boot-gating comment currency, and no unguarded `unwrap()`/panic risk in the PACK/PSDT/PLDT/CTDA parse arms or `sandbox_seat_system`'s hot path.

## Note on scope

This run covered only Dimension 9 per `--focus 9`. No baseline bench numbers (entity/draw/FPS/fence/parse-rate) were pulled or compared — that table is out of scope for a single-dimension AI-package run and should be sourced from the full `/audit-fnv` run instead.
