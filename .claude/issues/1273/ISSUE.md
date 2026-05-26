# F-FO3-D3-03: NPC_/CREA SCRI script-attachment field not extracted (24% of FO3 NPCs affected)

## Severity: Low (documented gap, quantified)

**Location**: `crates/plugin/src/esm/records/actor.rs:445-712` (`parse_npc`); same gap on CREA parser.

## Problem

`parse_npc` reads RNAM / CNAM / VTCK / SNAM / CNTO / PKID / DOFT / INAM / TPLT / ACBS / FGGS / FGGA / FGTS / HCLR / HNAM / LNAM / ENAM / PNAM / face-morph block (FO4+) — but no `SCRI` arm. `EsmIndex::base_record_script()` at `index.rs:516-538` walks `activators / containers / terminals / items` (every typed-map record that captures `script_form_id`); NPC_ and CREA are absent because the parser never stores SCRI on them.

The gap is already documented at `crates/plugin/src/esm/records/index.rs:504-509`:

> NPC_ / CREA — `NpcRecord` doesn't have a top-level `script_form_id` field today

This finding quantifies the FO3 footprint so the M47.0 follow-up is sized correctly.

## Evidence

Raw sub-record histogram on Fallout3.esm:

```
[FO3] NPC_=1647 (SCRI on  398 = 24%) | CREA=533 (SCRI on 148 = 27%) | ACTI=774 (SCRI on 616 = 79%)
[FNV] NPC_=3816 (SCRI on 1046 = 27%) | CREA=1578 (SCRI on 260 = 16%) | ACTI=1143 (SCRI on 992 = 86%)
```

ACTI captures SCRI correctly; NPC_ and CREA do not. **24–27% of FO3 actors author an attached script that the parser silently drops** (398 NPCs + 148 creatures = 546 actors on FO3 alone).

## Impact

Cannot resolve NPC-attached SCPT references at runtime. Consumer-side this manifests when the M47.0 actor AI hooks try to look up `npc.script_form_id` — there is no field. Per-NPC reactive behaviour (custom dialogue triggers, faction reactions tied to per-NPC scripts) is unreachable. Half of FO3's named NPCs ship custom logic — Three Dog's broadcast triggers, Moira Brown's questline gates, Megaton settler greetings, etc. Same impact on FNV's 1,046 SCRI-bearing NPCs.

## Fix

Add `pub script_form_id: u32` to `NpcRecord` at `crates/plugin/src/esm/records/actor.rs:~125`, then add an arm in `parse_npc` near the other sub-record arms:

```rust
b"SCRI" if sub.data.len() >= 4 => {
    record.script_form_id = SubReader::new(&sub.data).u32_or_default();
}
```

Mirror the change on the CREA parser. Also extend `EsmIndex::base_record_script()` to walk `npcs` and `creatures` (`index.rs:524-537`). Update the coverage-gap comment at `index.rs:506-509` once shipped.

Belongs to **M47.0 actor-AI follow-ups**, not an isolated fix bundle — the consumer side (where `script_form_id` lookups would actually fire) is the milestone that needs the field. Filing now so M47.0 sizing reflects the +1647+533 actor SCRI population.

## Completeness Checks

- [ ] **TESTS**: Regression test against an FO3 NPC fixture with SCRI present (Three Dog or Moira) asserts `script_form_id != 0`
- [ ] **SIBLING**: CREA parser gets the same arm
- [ ] **INDEX**: `EsmIndex::base_record_script()` updated to cover npcs + creatures
- [ ] **DOC**: Comment at `index.rs:504-509` updated/removed once the field exists
- [ ] **CROSS-GAME**: Skyrim NPC_ also carries SCRI in some records — verify same code path serves it

Audit: `docs/audits/AUDIT_FO3_2026-05-25_DIM3.md` (F-FO3-D3-03)
