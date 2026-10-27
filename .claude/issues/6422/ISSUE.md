# CHAR-2026-10-26-M-01: VATS `queue`/`confirm` never check AP; an unarmed scan prices every part at `f32::MAX`; same-target shots overwrite each other

- **Severity**: MEDIUM
- **Dimension**: Population Boundary / Progression & Pools
- **Game**: fo3, fnv
- **Location**: `byroredux/src/systems/vats.rs:358` (`ap_cost` fallback), `:486-490` (`vats_queue_part`), `:508-557` (`vats_confirm`), the two `vats_scan` input constructions (`concentrated_shot: 1`); test `confirm_spends_reserved_ap_and_leaves_the_mode` (`:787-830`)
- **Status**: NEW (sits inside open umbrella #6350, whose phase list has no affordability item)
- **Source**: the invariant is vanilla "queued shots may not exceed current AP". The Concentrated Fire part is GECK *VATS Settings* `iVATSConcentratedFireBonus` ("63 % … 68 on the second shot, 73 on the third").
- **Description**: The new live AP pool has a spend side with no gate. The orchestrator re-read the code and confirmed:
  1. `vats_scan` sets `ap_cost = weapon.map_or(f32::MAX, |w| w.ap_cost)`. The scan's own doc says a weaponless scan yields nothing, but it does not skip.
  2. `vats_queue_part` does `vats.ap_reserved += part.ap_cost` with no comparison to the player's current AP.
  3. `vats_confirm` does `avs.apply_damage(ap_avif, reserved.max(0.0))`, with no insufficient-AP early return.
  4. `HitEvent` is stored in a `SparseSetStorage` (one component per entity, `scripting/events.rs:84`). `events.insert(part.entity, …)` inside the per-part loop therefore lets a second queued shot at the same target overwrite the first. At most one hit lands per target per confirm, while all AP is still charged.
  5. `concentrated_shot` is hard-coded to 1, so `iVATSConcentratedFireBonus` is unreachable at runtime although `to_hit_chance` implements it.
  6. `damage` is computed once from `queue[0].entity` and reused for every target.
  7. A weapon with no skill reads the absent-AV default `0.0` rather than the neutral 50.
- **Evidence**: two unarmed parts give `f32::MAX + f32::MAX = +inf`. Armed, the unit test pins `95 − 8·17 = −41` as intended behaviour. FNV regen restores 3.6/s, so the debt takes a long time to drain.
- **Impact**: The player's saved `ActorValues` AP damage is wrecked until `setav`/`modav`. The HUD AP bar goes hugely negative, and shots still roll. It is reachable today only through the `vats.queue`/`vats.confirm` console and smoke driver. It becomes player-reachable the moment a VATS menu is wired to `vats_queue_part`. Multi-shot VATS under-delivers damage, and a perk-facing GMST is inert. The #6403 ground-truth capture cannot catch any of this.
- **Related**: #6350, #6403.
- **Suggested Fix**: Refuse a queue when `ap_reserved + part.ap_cost` exceeds `ActorValues::current(AP)`, and drop weaponless targets in the scan. Re-check on confirm and return `false` on insufficient AP. Accumulate or sequence same-target hits. Pass the 1-based repeat count to `to_hit_chance`. Price damage per target. Flip the 95−136 test to assert refusal.

### LOW

---
Source: `docs/audits/AUDIT_CHARACTER_2026-10-26.md` (HEAD bf304851f), finding `CHAR-2026-10-26-M-01`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / profiles / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)

