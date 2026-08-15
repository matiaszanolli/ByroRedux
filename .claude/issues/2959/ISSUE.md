# CHAR-D6-02: charal.md — the layer spec — is stale in four verifiable places, and omits the shipped regen module entirely

- **Issue**: [#2959](https://github.com/matiaszanolli/ByroRedux/issues/2959)
- **Finding ID**: `CHAR-D6-02`
- **Labels**: `medium,legacy-compat,documentation`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2959 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Coverage & Doctrine
- **Game**: all (FO4 and Skyrim specifically)
- **Location**: `docs/engine/charal.md` §4, §5, §8 item 3, §9 item 1
- **Status**: NEW
- **Source**: `docs/engine/charal-fo4-ruleset.md`, section "NPC SPECIAL storage —
  RESOLVED (xEdit `Core/wbDefinitionsFO4.pas`, dev-4.1.6)" — the capture that closes
  `charal.md` §9's first open-research item
- **Description**: `charal.md` is the authority for the layer's shape and its
  remaining work. Four of its claims are contradicted by the current tree:
  1. **§5 states `skyrim_ruleset` ships "an **empty derived table**"** (with a
     parenthetical rationale that Health/Magicka/Stamina aren't attribute-derived).
     `skyrim_ruleset` pushes **two** formulas: an Armor Rating multiplier
     (`LIGHT_ARMOR_RATING_COEFF`, player-only) and Carry Weight
     (`CARRY_WEIGHT_BIAS` + `CARRY_WEIGHT_STAMINA_COEFF`, base-layer only). The
     rationale is still correct for the *pools*; the "empty" claim is not.
  2. **§8 item 3 states FO4 NPC *population* "is unstarted"**, naming "PRPS property
     pairs vs. `RACE`/template inheritance vs. both" as the open question. Steps 1–2
     of the capture's own implementation path shipped: `derive_stored_actor_values`
     reads `npc.actor_value_props` (the PRPS pairs) plus the baked `DNAM`
     `calculated_health` / `calculated_action_points`, gated on
     `GameKind::uses_actor_value_properties`, with wire-level tests in
     `crates/plugin/src/esm/records/actor/tests.rs`. Only step 3 (RACE/template
     inheritance fallback) remains open — a much narrower gap than "unstarted".
  3. **§9's first open-research item** ("FO4 NPC SPECIAL storage … Research was in
     flight when CHARAL was proposed; resume before implementing FO4") is closed by
     `charal-fo4-ruleset.md`'s own **RESOLVED** section, which gives the authoritative
     xEdit definition. The spec's open-questions list still carries a question its
     own child document answered.
  4. **The `regen` module appears nowhere in `charal.md`.** `grep -i regen` over the
     spec returns zero hits, while `crates/core/src/character/regen.rs` ships
     `PoolRegenAccumulator`, `PoolRegenConfig`, `pool_regen_tick_system`,
     `POOL_REGEN_DT`, `FATIGUE_REGEN_PER_SEC`, `MAGICKA_REGEN_BASE`,
     `MAGICKA_REGEN_WILLPOWER_COEFF`, and `magicka_regen_per_sec`, and the tick is
     registered in `byroredux/src/boot.rs`. A shipped module carrying sourced numeric
     constants *and* the layer's only fixed-timestep system is documented only in
     `charal-oblivion-ruleset.md` and a `boot.rs` comment — never in the spec that
     §4 presents as the canonical component inventory.
- **Evidence**: §5 "with an **empty derived table**" vs. the two `rs.push_derived(…)`
  calls in `skyrim_ruleset`. §8 item 3 "is unstarted" vs. `derive_stored_actor_values`
  in `crates/plugin/src/esm/records/actor_value_derive.rs`. §9 item 1's "Research was
  in flight" vs. the capture's "**Answer: the `PRPS` (Properties) subrecord**".
  `grep -c -i regen docs/engine/charal.md` → 0.
- **Impact**: The spec understates what shipped in two places and overstates the open
  work in two more. Its §8/§9 lists are what a milestone-planner reads to decide what
  to build next; both currently point at work that is done. The `regen` omission is
  the more structural one — the next contributor to touch pool regeneration has no
  entry point from the layer doc.
- **Related**: `CHAR-D6-01` (same omission of `regen`'s wiring status from the crate
  docstring); `CHAR-D6-03`; `CHAR-D3-06` (Skyrim/Oblivion constants sourced only to
  `charal.md` prose — this finding is why that circular sourcing is fragile).
- **Suggested Fix**: Correct the §5 Skyrim sentence to "two derived rows, no
  attribute-derived pools"; narrow §8 item 3 to the RACE/template fallback; strike §9
  item 1 with a pointer to the capture's RESOLVED section; add a §4.7 for `regen`
  recording the mechanism, its constants, and that it is registered-but-no-op.

---

## Completeness Checks
- [ ] **SIBLING**: The same drift class is swept across the other capture documents / docstrings, not just the row cited
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
