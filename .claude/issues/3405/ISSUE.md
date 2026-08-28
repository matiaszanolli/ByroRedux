# #3405 — ESM-2026-08-27-D1-03: GameKind::from_header's sampled-value table says FO76 HEDR is 68.0; both installed FO76 masters ship 266.0

**Labels**: low, esm-plugin, documentation, doc-rot
**Source**: `docs/audits/AUDIT_ESM_2026-08-27.md`

---

**Audit**: `docs/audits/AUDIT_ESM_2026-08-27.md` (`/audit-esm`, deep, tree `main` @ `969d81c8`)
**Severity**: LOW · **Dimension**: Header Detection (documentation)
**Record / Sub-record**: `TES4` / `HEDR`
**Location**: `crates/plugin/src/esm/reader.rs` — the sampled-values comment block inside `GameKind::from_header`, and the `hedr_version >= 60.0` band it justifies

## Description

The comment block that justifies every band in `from_header` lists *"FO76 = 68.0"* among "HEDR versions sampled from real vanilla masters at 2026-04-19". Reading the TES4 header directly off disk gives `SeventySix.esm` = **266.0** and `NW.esm` = **266.0**. The band itself (`hedr_version >= 60.0 → Fallout76`) still classifies both correctly, so there is no misclassification — but the sampled values are the *evidence* the band gaps rest on, and one of the six is wrong by a factor of ~4.

## Evidence

Independent header read (not through this parser), all seven installed games:

```
Oblivion.esm    1.0    (20-byte header → EsmVariant::Oblivion)
Fallout3.esm    0.94   rec_ver 2
FalloutNV.esm   1.34   rec_ver 2      (DLC masters: 1.32 / 1.33)
Skyrim.esm      1.71   rec_ver 44
Fallout4.esm    1.0    rec_ver 131    (DLCs: 0.95 & 1.0, rec_ver 131)
SeventySix.esm  266.0  rec_ver 209
Starfield.esm   0.96   rec_ver 581
```

Every one of these classifies correctly through the current bands, re-derived arm by arm; the FNV DLC values 1.32/1.33 and the FO4 0.95 + `rec_ver >= 100` discriminator both behave as documented.

## Impact

Documentation only. It matters because the next person widening or narrowing a band will reason from this table.

## Related

`#439` / FO3-3-01 (the inverted-band incident this comment block exists to prevent recurring).

## Suggested Fix

Correct the line to `FO76 = 266.0` and note that the `>= 60.0` floor is deliberately far below it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other sampled-value tables justifying version bands)
- [ ] **TESTS**: A regression test pins this specific fix (a `from_header(Tes5Plus, 266.0, 209) == Fallout76` assertion documents the real value)
