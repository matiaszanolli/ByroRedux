# RT-9: SDK faction catalog rejects vanilla Oblivion's -200 XNAM edges

Labels: low, tech-debt, bug, game:oblivion
Source: docs/audits/AUDIT_RUNTIME_2026-10-27.md

**Report**: `docs/audits/AUDIT_RUNTIME_2026-10-27.md` · **Finding**: RT-9 · **Severity**: LOW · **Dimension**: Runtime log review (SDK; owner `/audit-tooling`)
**Location**: `crates/sdk/src/relationships.rs:57` (`if !(-100..=100).contains(&modifier)` in `FactionRelationship::new`); caller `byroredux/src/cell_loader/load_order.rs:277-290`
**Game**: Oblivion

## Description
Every Oblivion boot logs four `SDK faction relationship ... rejected: faction relationship modifier -200 is outside -100..=100`. A census of `Oblivion.esm` FACT `XNAM` finds exactly four edges with |modifier| > 100, all Shivering Isles crystal-war hostility:
- `SE08OrderFaction` (0005282C) -> `SE08GoldenSaintFaction` / `SE08DarkSeducerFaction`, -200
- both of those -> `SEOrderFaction` (000134D4), -200

The +-100 bound has no cited source. The CS wiki documents no range, and the vanilla master authors -200. The catalog drops these four edges and sets `truncated = true` (confirmed in `load_order.rs`: the `Err` arm sets `truncated` and logs a warn). Oblivion `XNAM` carries no combat reaction, so `faction_hostility` does not read the dropped modifier today. WASM mods do: the catalog is exposed to them (`crates/mod-runtime/src/runtime/host/faction_relationships.rs`).

## Evidence
Four WARN lines per Oblivion boot; the bound at `relationships.rs:57` verified at HEAD.

## Impact
Mods see the vanilla Shivering Isles relationship graph minus its strongest hostility edges, and the `truncated` flag reports a content error that does not exist.

## Related
SDK faction catalog (`crates/sdk`), `crates/mod-runtime`.

## Suggested Fix
Widen the bound to what the format carries (`i32`), or cite a source for a clamp. Pin the four vanilla edges in a real-data test.

## Completeness Checks
- [ ] **SIBLING**: FO3/FNV/Skyrim `XNAM` edges checked for the same bound (Skyrim `CRVA`/`XNAM`)
- [ ] **TESTS**: Real-data test pins the four vanilla -200 edges

