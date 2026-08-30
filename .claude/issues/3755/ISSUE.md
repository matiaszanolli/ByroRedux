# #3755: FO3-2026-08-30-D3-02: the ACRE comment says "Oblivion-only ... FO3+ folded creature placements into ACHR" — FO3 ships 3,349 ACRE, more than its 2,154 ACHR

**Labels**: documentation, low, legacy-compat, game:fo3, esm-plugin, doc-rot
**Filed**: 2026-08-30 (audit-publish)

---

**Report**: `docs/audits/AUDIT_FO3_2026-08-30.md` · **Severity**: LOW (doc-rot with a live hazard) · **Dimension**: 3 (ESM Record Coverage)
**Game affected**: Fallout 3 (and FNV, which shares the walker)

## Location
- `crates/plugin/src/esm/cell/walkers.rs` — the `ACRE` comment above the REFR/ACHR/ACRE arm (currently `:695-698`)

## Description
```rust
// ACRE — Oblivion-only placed-creature reference (#396). FO3+
// folded creature placements into ACHR; ACRE's wire layout
// matches ACHR byte-for-byte on Oblivion (NAME/DATA/XSCL/XESP),
// so it routes through the same handler.
```

Measured on `Fallout3.esm`: **`ACRE` 3,349, `ACHR` 2,154**. FO3 did **not** fold creature placements into `ACHR`. It splits them exactly the way Oblivion does — `CREA` placements to `ACRE`, `NPC_` placements to `ACHR`. FO3 ships *more* ACRE than ACHR.

Re-verified 2026-08-30: the comment is unchanged in current source.

## Evidence
The Dimension-3 reconciliation only closes because `ACRE` *is* accepted:

```
568,107 REFR + 2,154 ACHR + 3,349 ACRE = 573,610 = the indexed placed-ref total
```

Remove ACRE and the parser loses 3,349 placements.

## Impact
**No behavioural defect today** — the arm is ungated by game. The hazard is the premise. A reader trusting "Oblivion-only" would gate the arm on `GameKind::Oblivion` and silently delete 3,349 FO3 creature placements (super mutants, radroaches, brahmin, mirelurks) plus FNV's equivalent.

The project has already paid for this exact class of comment once — #1538.

## Related
#396 (the original ACRE arm), #1538 (the same class of premise-driven gate that dropped 98 FNV bases).

## Suggested Fix
Replace with the measured statement:

> "placed-creature reference. Oblivion, FO3 and FNV all author it (FO3: 3,349 `ACRE` vs 2,154 `ACHR`); the wire layout matches `ACHR` byte-for-byte, so it routes through the same handler."

## Completeness Checks
- [ ] **SIBLING**: check the other per-game "X-only" claims in the same walker against the corpus in the same pass
- [ ] **TESTS**: a floor in `parse_rate_fo3_esm` asserting ACRE placements ≥ 3,000 would make the premise machine-checked rather than prose
