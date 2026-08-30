# #3753: FO3-2026-08-30-D3-01 (second half): parse_proj never reads MODL and PROJ dispatches via extract_records, so PROJ bases are absent from cells.statics — #3542's PGRE arm alone still resolves to no mesh

**Labels**: bug, medium, legacy-compat, game:fo3, esm-plugin
**Filed**: 2026-08-30 (audit-publish)

---

**Report**: `docs/audits/AUDIT_FO3_2026-08-30.md` · **Severity**: MEDIUM · **Dimension**: 3 (ESM Record Coverage)
**Game affected**: Fallout 3 (and every game whose PROJ bases are placed)

> **Scope note.** The FO3 audit's D3-01 finding has two halves. The **first half** — the shared CELL walker has no `PGRE` arm, so all 350 FO3 placed mines are dropped — **is already filed as #3542** and is *not* re-filed here. This issue is the **second half**, which #3542 does not cover: even with a `PGRE` placement arm, the PROJ base records carry no mesh, so the placements would still resolve to nothing.

## Location
- `crates/plugin/src/esm/records/misc/effects.rs` — `parse_proj`
- `crates/plugin/src/esm/records/dispatch_misc_gameplay_b.rs` — the `b"PROJ"` dispatch arm

## Description
`parse_proj` never reads `MODL`, and the `PROJ` dispatch uses `extract_records`, **not** `extract_records_with_modl`. So `PROJ` bases never land in `cells.statics` and have no model path at all.

Verified against current source: `parse_proj`'s sub-record loop handles only `b"DATA"` (plus the shared `CommonNamedFields` for EDID/FULL); there is no `b"MODL"` arm. The dispatch reads:

```rust
b"PROJ" => extract_records(reader, end, b"PROJ", &mut |fid, subs| {
    index.projectiles.insert(fid, parse_proj(fid, subs));
})?,
```

## Evidence
Probed live on `Fallout3.esm`:

```
form 0x43fa: in statics=None  in projectiles=true
```

— the same for all four PROJ bases the FO3 `PGRE` records reference:

| form | EditorID | MODL | PGRE placements |
|---|---|---|---:|
| `0x0043FA` | `MineFragProjectile` | `Weapons\1handMineDrop\MineFrag.NIF` | 340 |
| `0x0403D8` | `MinePlasmaProjectile` | `Weapons\1handMineDrop\MinePlasma.NIF` | 7 |
| `0x033C4B` | `MinePulseProjectile` | `Weapons\1handMineDrop\MinePulse.NIF` | 2 |
| `0x059449` | `MineBottleCapProjectile` | `Weapons\1handMineDrop\MineBottleCap.NIF` | 1 |

All four meshes exist in `Fallout - Meshes.bsa` and parse clean (63,965 / 298,492 / 95,023 / 125,121 bytes, `parse_nif` OK). The MODL data is present on disk and is simply not read.

## Impact
This is the half that makes #3542 insufficient on its own. **Adding a `PGRE` placement arm alone would still resolve to no mesh** — the placements would be decoded and then dropped at spawn time for want of a model path. Both halves are required for a single placed FO3 mine to render.

Beyond FO3 mines, every PROJ base in every game is currently a model-less record, so any future projectile/hazard system reading `cells.statics` finds nothing.

## Related
**#3542** (the `PGRE`/`PHZD` cell-walker half — the necessary companion fix); `docs/audits/AUDIT_ESM_2026-08-13.md` skipped-record table; #1538 (the structurally identical `is_fo4_plus` SCOL gate that dropped 98 FNV bases).

## Suggested Fix
Switch the `PROJ` dispatch to `extract_records_with_modl` so the four base meshes land in `cells.statics`, and add a `b"MODL"` arm to `parse_proj`. Land alongside #3542 — neither half renders anything on its own. Add a floor to `parse_rate_fo3_esm` asserting the PROJ bases carry a model path so the arm cannot silently regress.

## Completeness Checks
- [ ] **SIBLING**: other `extract_records` dispatches whose records carry a `MODL` the engine needs — audit the dispatch table in the same pass
- [ ] **TESTS**: a regression test asserting the four FO3 PROJ bases resolve `in statics`
