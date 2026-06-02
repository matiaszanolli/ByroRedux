# Issues 1391 + 1396

## #1391 EGUI-05: EguiPassConfig is dead code
**File:** `crates/debug-ui/src/lib.rs:208`
**Fix:**
- Remove `EguiPassConfig` struct (never constructed or referenced outside its own definition)
- Remove `use std::sync::Arc` (sole usage was the struct's `allocator` field)
- Remove `ash` and `gpu-allocator` from `crates/debug-ui/Cargo.toml`

## #1396 SAFE-U1: Stale doc comment on BuiltinType implies transmute
**File:** `crates/sfmaterial/src/types.rs:8-10`
**Fix:** Replace "transmute into this enum via `BuiltinType::from_u32`" with accurate
description — `from_u32` is a fully checked match that returns `Err(UnsupportedBuiltin)`
for unknown tags. No unsafe code is involved.
