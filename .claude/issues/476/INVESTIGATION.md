# Investigation — Issue #476 (FNV-3-L1)

## Domain
ESM — `crates/plugin/src/esm/records/climate.rs` + `byroredux/src/cell_loader.rs` (consumer).

## Root cause

`ClimateRecord.weathers[i].chance` declared `u32` at `climate.rs:16`, parsed via `read_u32_at` at `climate.rs:74`. UESP + in-code comment at `climate.rs:65` both say `i32`. Negative chance (mod sentinel) wraps to huge positive `u32` and wins `max_by_key` in `cell_loader.rs:555`.

## Fix

1. `climate.rs:16` — change `chance: u32` → `chance: i32`.
2. `climate.rs:62-83` — cast the `read_u32_at` result via `as i32` (lossless bit reinterpretation of little-endian signed).
3. `cell_loader.rs:555` — filter `chance >= 0` before `max_by_key` so negative sentinels don't win.
4. Tests at `climate.rs:121,124,139,141` — update literal types from `u32` to `i32` and keep the numeric values (60, 40) which are valid as i32. Add a new test pinning the negative-chance skip behavior.

## Scope
2 files. Changes are type-only (ABI-compatible bit layout).
