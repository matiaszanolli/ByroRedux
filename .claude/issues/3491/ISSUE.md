# Issue #3491 — SAVE-D1-2026-08-27-02: the `Perks` completeness-allowlist reason cites a `validate_progression_state` guard that never inspects `Perks`

Source audit: `docs/audits/AUDIT_SAVE_2026-08-27.md`
Filed: 2026-08-27 (HEAD `969d81c8`)
Labels: medium, save-load, character, test-gap, bug

---

Audit: `docs/audits/AUDIT_SAVE_2026-08-27.md` (SAVE-D1-2026-08-27-02)
Severity: **MEDIUM** · Dimension 1 — Snapshot Completeness & Determinism
Data-Loss Class: latent silent-drop (no loss today — `Perks` has zero production mutators)

## Location
- `byroredux/src/save_io/registry_completeness_tests.rs:108` — the allowlist reason
- `byroredux/src/save_io/round_trip_tests.rs:757`, `:781` — the `REDERIVED_NOT_SAVED` preamble repeating the claim
- `crates/save/src/validate.rs:410-442` — `validate_progression_state`, which never mentions `Perks`

## Description
The SAVE-D1-12 allowlist entry reads:

```rust
("Perks", "known progression gap guarded by validate_progression_state: saves are refused once perks exist (#2947)"),
```

and `round_trip_tests.rs`'s `REDERIVED_NOT_SAVED` preamble repeats the claim for both types: *"`crates/save/src/validate.rs::validate_progression_state` aborts any save where a `CharacterLevel.xp != 0` slips through with these two still unregistered, so the exemption fails loudly rather than silently discarding progress."*

`validate_progression_state` reads `Perks` nowhere. Its whole body is:

```rust
fn validate_progression_state(world: &World, errors: &mut Vec<ValidationError>) {
    let Some(q_level) = world.query::<CharacterLevel>() else { return; };
    for (entity, level) in q_level.iter() {
        if level.xp != 0 { … }
    }
}
```

Its own doc comment (`validate.rs:410-422`) is scrupulously accurate and only ever discusses `CharacterLevel.xp` — the over-claim exists solely in the two allowlist reasons. The stated trigger ("once perks exist") is also already false in the component sense: `scene.rs:1393` inserts `Perks::default()` on the player body unconditionally (#3158), and `npc_spawn.rs:178-186` inserts a populated `Perks` for any NPC with `PRKR` entries. Saves are not refused, correctly, because `Perks` is genuinely write-once from ESM today — but that is a *different* argument from the one the allowlist makes.

## Evidence
- `crates/save/src/validate.rs:424-442` — the complete function body, quoted above; `grep -rn "Perks" crates/save/src/` returns **zero** hits, so the only `Perks` reference anywhere in `crates/save` is absent.
- `grep -rn "get_mut::<Perks>\|query_mut::<Perks>\|resource_mut::<Perks>" byroredux/src crates/` returns zero hits, confirming the exemption is *substantively* safe today.
- `byroredux/src/scene.rs:1393` — `world.insert(body, byroredux_core::character::Perks::default());`.

## Impact
The completeness ledger is this subsystem's primary silent-drop defence, and the audit skill's own Dimension 1 instruction is to consume its reasons rather than re-derive them — so a wrong reason propagates directly into every future audit and every future reviewer's mental model. The day an `AddPerk` effect or a perk-selection UI lands (`docs/engine/charal.md`'s perk work, #3004/#2986), the author will read "guarded by `validate_progression_state`", conclude the gate will catch them, and ship a runtime that silently discards every granted perk on save — with the guard test still green, because a green `NOT_SAVED_BY_DESIGN` entry only asserts that *a* reason exists, not that it is still true.

## Related
#2947 (the `CharacterLevel` half, which is correctly implemented); #3158 (the unconditional player `Perks` stub); the audit skill's own note that the guard "enforces a reason exists, not that it's still true".

## Suggested Fix
Either extend `validate_progression_state` to also flag a non-empty `Perks` on any entity (making the claim true and giving the future perk runtime the loud failure the ledger promises), or rewrite both reasons to state the real justification — "`Perks` is stamped verbatim from `NPC_.PRKR` at spawn with no production mutator (`grep` for `get_mut::<Perks>`); register it the moment an `AddPerk` effect lands." Prefer the former: it costs four lines and makes the ledger self-enforcing rather than self-describing.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — every other `NOT_SAVED_BY_DESIGN` / `REDERIVED_NOT_SAVED` reason that names a guard, checked against what that guard actually inspects
- [ ] **TESTS**: A regression test pins this specific fix
