# #3404 — ESM-2026-08-27-D2-01: rows() silently drops a non-zero stride remainder, unlike every other decoder in the same file

**Labels**: low, esm-plugin, bug
**Source**: `docs/audits/AUDIT_ESM_2026-08-27.md`

---

**Audit**: `docs/audits/AUDIT_ESM_2026-08-27.md` (`/audit-esm`, deep, tree `main` @ `969d81c8`)
**Severity**: LOW · **Dimension**: Sub-Record Byte Accounting
**Record / Sub-record**: `NAVM` / `NVVX`, `NVTR`, `NVEX`, `NVDP`, `NVCA`; `REGN` / `RDWT`
**Location**: `crates/plugin/src/esm/records/misc/world.rs` (`fn rows`)

## Description

Every repeating-row sub-record in `world.rs` goes through

```rust
fn rows(data: &[u8], stride: usize) -> impl Iterator<Item = &[u8]> {
    data.chunks_exact(stride.max(1))
}
```

`chunks_exact` discards the remainder with no signal. The file's own posture everywhere else is the opposite: `decode_nvgd` and `decode_nvnm` both return `None` unless the payload is consumed *exactly*, with the rationale written out in the module — *"the same all-or-nothing posture … so a format revision surfaces as 'not recognised' rather than as a half-filled lattice"*. `decode_weather_rows` even has an `exact` flag doing the remainder check by hand. `rows()` is the one path with neither.

## Evidence

The `#3300` doc comments state the strides were established by taking the GCD of observed payload lengths across 11,969 shipped FO3+FNV meshes and cross-checking against `DATA` words 1–5, so on shipped data every remainder is zero today. That is exactly the condition under which a silent drop is invisible: the first game (or the first mod) whose `NVDP` stride is not 8 will decode a truncated door list and report success.

## Impact

No known incorrect output on shipped data. The gap is diagnostic: a stride that is wrong for a future game degrades to partial data with no error, no log and no counter.

## Related

`#3300`; the `/audit-esm` Dimension 2 repeating-row checklist item.

## Suggested Fix

Give `rows()` a strict sibling (`rows_exact` returning `Option`) and use it wherever a `DATA`-header count is available to cross-check — `NVDP`/`NVCA` already have one (`DATA` words 4 and 5).

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other `chunks_exact` row decoders across `records/`)
- [ ] **TESTS**: A regression test pins this specific fix
