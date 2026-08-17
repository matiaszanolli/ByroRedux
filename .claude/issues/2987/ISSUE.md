# ESM-2026-08-16-D7-02: health_actor_value_key returns an engine enum index in a FormID-keyed map

**Issue**: #2987
**Severity**: HIGH
**Dimension**: 7 — EsmIndex → ECS Handoff
**Labels**: `high,import-pipeline,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_ESM_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_ESM_2026-08-16.md` (Dimension 7 — `EsmIndex` → ECS Handoff).

**Record / Sub-record**: `AVIF`
**Location**: `crates/plugin/src/esm/records/index.rs`:365-372 (`SKYRIM_HEALTH_ACTOR_VALUE`), :592-604 (`health_actor_value_key`) · `crates/plugin/src/esm/records/actor_value_derive.rs`:134-152 (`derive_skyrim_actor_values`)

## Description

The constant's doc says:

> Skyrim addresses its built-in actor values by engine enum index rather than by an `AVIF` record. Vanilla `Skyrim.esm` therefore does not need to contain a `Health` AVIF for NPC health to be usable.

**Vanilla `Skyrim.esm` contains 149 `AVIF` records and one of them is `AVHealth`, FormID `0x000003E8`.** The lookup fails not because the record is absent but because of the `AV`-prefix mismatch in D7-01 (#2986).

The chosen workaround puts a bare enum index (`24`) into `ActorValues`, whose contract (`crates/core/src/ecs/components/actor_values.rs`:64-77, and the `actor_value_form_id` docstring) is that keys are **remapped global AVIF FormIDs — "the same space a remapped CTDA `param_1` … compares against"**.

## Evidence

Probe output and an independent raw `AVIF` GRUP walk both show `0x3e8 AVHealth` in `Skyrim.esm`.

The two spaces are provably **disjoint at the consumer**: `param1_is_form_id` (`crates/plugin/src/esm/records/condition.rs`:403-423) lists `| 14  // GetActorValue — AVIF FormID`, so every Skyrim CTDA `GetActorValue` arrives at `crates/scripting/src/condition.rs`:434 with `param_1 = remap(0x000003E8)`, while the map it indexes holds exactly one key, `24`.

Measured: **5,085 of 5,118 `Skyrim.esm` NPCs carry exactly one derived pair, all keyed `24`.**

Re-verified 2026-08-17: `SKYRIM_HEALTH_ACTOR_VALUE: u32 = 24` and the `matches!(game, GameKind::Skyrim)` short-circuit in `health_actor_value_key` are both present and unchanged.

## Impact

Every Skyrim `GetActorValue(AVHealth)` condition silently reads the absent-AV default `0.0` — quest gates, AI package conditions, and perk entry conditions on actor health are all evaluated against a value the actor **does** have, but under a key nothing else uses.

Combat is self-consistent (it reads the key back out of `ActorVitals`), which is precisely why the P2 gate is green: `docs/smoke-tests/p2-melee-core.sh`:99 runs the melee-core check on `Skyrim.esm` — the one game whose actor values come from this bypass.

The divergence is also **serialized**: `ActorValues` keys go to disk through `byroredux/src/save_io.rs`:249's registry as plain `u32`, so existing saves carry the wrong key space.

## Suggested Fix

Delete the special case once D7-01 (#2986) lands, and resolve Skyrim Health through `AVHealth` like every other actor value.

If a built-in enum key is genuinely wanted later, it needs its **own key type**, not a `u32` sharing a FormID map.

Note the save-migration angle: keys already written to disk are in the enum space, so the fix needs either a migration or an explicit decision to invalidate those saves.

## Related

- **Same root cause as #2986 (ESM-2026-08-16-D7-01) — fix them together**
- The false-premise-in-a-comment shape is the `feedback_no_guessing` pattern

## Completeness Checks
- [ ] **SIBLING**: Any other per-game short-circuit in `index.rs` checked for a similarly false premise
- [ ] **KEY-SPACE**: `ActorValues` keys are FormIDs everywhere, or a distinct key type is introduced — never a `u32` union of two spaces
- [ ] **SAVE-MIGRATION**: Existing saves carrying enum-space keys are migrated or explicitly invalidated
- [ ] **SMOKE**: `docs/smoke-tests/p2-melee-core.sh` still passes, and no longer passes *because of* the bypass
- [ ] **TESTS**: A regression test pins Skyrim Health resolving to `0x3E8`

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 2987 --json state` when live state is needed.*
