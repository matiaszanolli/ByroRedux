### COORD-01: `--rotation-mode` CLI dispatcher defeats its own out-of-range safety fallback

- **Severity**: MEDIUM
- **Dimension**: Legacy compatibility — Coordinate-system correctness (Z-up → Y-up)
- **Location**: `byroredux/src/boot/mod.rs:293-303`
- **Status**: NEW
- **Description**: `crates/core/src/math/coord.rs::euler_zup_to_quat_yup_mode` is documented and pinned (`out_of_range_mode_falls_back_to_ship`) to fall back to the shipping ZYX/CW formula (mode 1) for any mode outside `0..=3`, specifically "so a bad `--rotation-mode` argument can't produce garbage placement." The CLI entry point never lets that fallback fire: `crate::cell_loader::set_refr_rotation_mode_diag(mode.min(3))` pre-clamps any out-of-range value to **3** — one of the two explicitly-wrong diagnostic-only conventions (CCW + XYZ-product) — before it reaches the library's own guard. The adjacent comment ("Defaults to 0 (current shipping behavior)") is also stale: the shipping default has been mode 1 since the 2026-05-26 ZYX fix (`REFR_ROTATION_MODE_SHIP = 1`, `cell_loader/euler.rs`).
- **Evidence**:
  ```rust
  // boot/mod.rs:297-301
  // on a known-good cell. Defaults to 0 (current shipping behavior).
  if let Some(idx) = args.iter().position(|a| a == "--rotation-mode") {
      if let Some(mode) = args.get(idx + 1).and_then(|v| v.parse::<u8>().ok()) {
          crate::cell_loader::set_refr_rotation_mode_diag(mode.min(3));
  ```
  vs. `coord.rs`'s contract: `_ => Quat::from_rotation_y(-rz) * Quat::from_rotation_z(ry) * Quat::from_rotation_x(-rx)` for any mode not in `{0,2,3}`, pinned by `out_of_range_mode_falls_back_to_ship` for modes `4, 9, 255`. Confirmed live at HEAD: `REFR_ROTATION_MODE_SHIP: u8 = 1` (`crates/core/src/math/coord.rs:160`), and `boot/mod.rs` still reads `mode.min(3)` before calling `set_refr_rotation_mode_diag`.
- **Impact**: Narrow blast radius — only reachable via the opt-in `--rotation-mode` diagnostic flag, never the default game path (`set_refr_rotation_mode_diag` is only called when the flag is explicitly passed). But an operator using this exact tool to triage a placement bug (e.g. `--rotation-mode 4`) is silently steered into the wrong CCW+XYZ convention instead of the safe ship default, undermining the triage session the flag exists for. The stale "Defaults to 0" comment could also mislead a future engineer about current shipping behavior.
- **Related**: Originated in `196dd67c8` (2026-05-07). Survived the `main.rs → boot.rs` (`#1858`) and `boot.rs → boot/` (`#3855`) moves unchanged. Not caught by `AUDIT_LEGACY_COMPAT_2026-08-27.md`'s Dimension 1 sweep (which checked only for hardcoded modes/re-derived formulas, not this clamp). No matching open/closed GitHub issue found.
- **Suggested Fix**: Replace `mode.min(3)` with a straight pass-through (`set_refr_rotation_mode_diag(mode)`), letting `euler_zup_to_quat_yup_mode`'s own `_ =>` arm provide the fallback; update the stale "Defaults to 0" comment to "Defaults to 1 (current shipping behavior)".

## Completeness Checks
- [ ] **SIBLING**: Check for any other CLI diagnostic flag that pre-clamps a value before handing it to a library function with its own documented fallback
- [ ] **TESTS**: A regression test pins that an out-of-range `--rotation-mode` argument reaches the library's `_ =>` fallback (mode 1), not a pre-clamped value
