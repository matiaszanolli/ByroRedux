# ESM-2026-10-31-01: FO4 `EXPL` `DATA` is decoded at the form-version-97 layout for every record

Labels: low,esm-plugin,bug,game:fo4

Source: `docs/audits/AUDIT_ESM_2026-10-31.md` (ESM-2026-10-31-01). Severity LOW. Dimension: Sub-Record Byte Accounting. Record: `EXPL` / `DATA` (FO4).

**Location**: `crates/plugin/src/esm/records/misc/effects.rs:447-458` (`parse_expl`, the `Skyrim | Fallout4` arm; landed in `c36ab4125`)

## Description
- The arm reads `Fallout4` as damage @28, Inner Radius @32, Outer Radius @36.
- The older xEdit (`/mnt/data/tmp/wbDefinitionsFO4.pas`) gates the FO4 Inner Radius with `wbFromVersion(97, ...)`. xEdit `dev` drops that gate and gates only `Vertical Offset Mult` at form version 99.
- The data agrees with the version gate. Of 238 `Fallout4.esm` EXPL, 18 have a `DATA` shorter than the 84-byte full form; 15 of them are below form version 97 and carry no Inner Radius, so Outer Radius sits at @32.

## Evidence (floats @24..@44)
| Record | Form version | DATA size | Floats |
|---|---|---|---|
| FakeForce300Explosion | 63 | 52 | 300, 0, **512**, 0, 0.1 |
| FakeForce200Explosion | 88 | 56 | 200, 0, **128**, 0, 0.1 |
| CarNukeTEST | 95 | 60 | 20000, 200, **550**, 10000, 0.3 |
| MirelurkHunterSpitExplosion | 99 | 64 | 0, 0, 0, **200**, 0 |

The code stores 512 / 128 / 550 as `inner_radius` and leaves `radius` at 0 (or the IS radius).

## Impact
15 vanilla EXPL have a wrong `radius` and `inner_radius` holds the outer value. `systems/mines.rs` takes `blast.radius.max(blast.inner_radius)`, which masks the 0-radius cases; only mine explosions are consumed and vanilla mines are modern forms, so there is no live symptom.

## Suggested Fix
Infer the Inner Radius word from the record length (`DATA` >= 64 bytes present; 52-60 absent), or thread the record form version into `parse_expl`. Add a fixture for a 52-byte record.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
