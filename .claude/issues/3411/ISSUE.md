# #3411 — RT-2026-08-27-04: fo4 and skyrim_se entity + skin-pool counts move past their gates; bisected to #3357's multi-ARMA armor resolve

Labels: medium, esm-plugin, game:fo4, game:skyrim, bug
Filed: 2026-08-27 by `/audit-publish docs/audits/AUDIT_RUNTIME_2026-08-27.md`
Source report: `docs/audits/AUDIT_RUNTIME_2026-08-27.md`

---

Source: `docs/audits/AUDIT_RUNTIME_2026-08-27.md` — RT-2026-08-27-04 (live headless runs at `969d81c8`).

- **Severity**: MEDIUM
- **Dimension**: runtime telemetry → NPC equip
- **Games**: `fo4` (`InstituteBioScience`), `skyrim_se` (`WhiterunDragonsreach`)
- **Location**: `crates/plugin/src/equip.rs` (`resolve_armor_meshes`, pass 1, ~:192-212); consumers `byroredux/src/npc_spawn.rs:803-817` and `:918-931`
- Attribution of a gate move, not a claim that the fix is wrong.

## Description

Four gated metrics moved outside contract:

| Metric | fo4 | skyrim_se |
|---|---|---|
| `entities_total` | 18256 → 20154 (+10.4 %, band ±2 %) | 8126 → 9363 (+15.2 %) |
| `skin_pool_live` | 248 → 349 (+40.7 %, gate ≤ baseline) | 83 → 133 (+60.2 %) |

A probe at `fa71f1a2` (`e0d5ec18^`) isolates the cause cleanly:

```
fo4 InstituteBioScience   fa71f1a2  entities=18506  skin=278  armor_meshes=76
                          969d81c8  entities=20154  skin=349  armor_meshes=147
skyrim WhiterunDragonsreach fa71f1a2 entities= 8685  skin=108  armor_meshes=39
                          969d81c8  entities= 9363  skin=133  armor_meshes=58
```

On `fo4` the armor-mesh delta (+71) and the `skin_pool_live` delta (+71) are **identical**, so #3357 accounts for the skinned-mesh rise exactly. `fnv`, `fo3` and `oblivion` are untouched — `resolve_armor_meshes` short-circuits to the single-`MODL` path for pre-Skyrim games (`equip.rs:169-178`), which is why those three baselines reproduce perfectly.

## Evidence

The per-actor distribution is where this stops being obviously benign. On `fo4 InstituteBioScience`:

```
fa71f1a2:  13 actors x1    12 actors x2    1 actor x7     4 actors  x8
969d81c8:  13 actors x1    12 actors x3    1 actor x18    4 actors x20
```

`InstM03LvlSynth` and `LvlSynth_Institute_Superbarrel` now equip 20 and 18 simultaneous armor meshes. Pass 1 returns every race-matching ARMA of an ARMO without consulting that addon's own biped region:

```rust
for &arma_fid in armatures {
    …
    let race_match = arma.race_form_id == race_form_id
        || arma.additional_races.contains(&race_form_id);
    if race_match { if let Some(path) = pick_path(arma) { … out.push(path) } }
}
```

and #2094's slot-occupancy `retain` (`npc_spawn.rs:977`) treats the whole group as one `inv_idx`, so a multi-part item's meshes are not subject to slot displacement. That is the same *shape* as the over-equip that `bfdc3d3f` removed from `fnv` on 2026-08-23 (29 meshes from 9 inventory entries), reached by a different mechanism. Whether 20 is correct for an Institute synth needs the ARMO/ARMA data and is out of scope for a telemetry pass — the audit reports the measurement and the mechanism, not a verdict.

## Impact

Two baselines are red on two metrics each. If #3357's counts are correct, both baselines need regeneration with the attribution recorded in the header (the `bfdc3d3f`/`fnv` precedent). If the ×2.5 per-actor rise on `fo4` is over-equip, `skin_pool_live` is carrying real waste — 101 extra skinned meshes on one interior, against a pool cap of 1364, at 40 % of the scene's entire skinned budget.

## Related

`e0d5ec18` (#3357), `bfdc3d3f`, #2094, #2093. `AUDIT_RUNTIME_2026-08-26.md` RT-2026-08-26-01 is the `fnv` precedent. #3402 is the downstream consequence on `skyrim_se`.

## Suggested Fix

Decide the correctness question first (does an FO4 ARMO's ARMA set legitimately cover 20 distinct regions for one actor?), then either regenerate both baselines with a header recording `e0d5ec18`, or gate pass 1 on the addon's own biped mask so a region already covered by a higher-priority item does not also spawn its ARMA mesh.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the FO76 / Starfield arms of the same `is_skyrim_or_later` branch)
- [ ] **TESTS**: A regression test pins this specific fix
