# FO4-D5-LOW-02b: parse_rate_fo4_all_meshes MeshesExtra clean-rate floor is an uncalibrated placeholder

**Severity**: LOW (INFO) · **Source**: AUDIT_FO4_2026-06-02 (D5-INFO-02)

**Location**: `crates/nif/tests/parse_real_nifs.rs:219-222`

## Description

The `parse_rate_fo4_all_meshes` test covers both `Fallout4 - Meshes.ba2` and `Fallout4 - MeshesExtra.ba2`. The MeshesExtra threshold:

```rust
ArchiveSpec {
    name: "Fallout4 - MeshesExtra.ba2",
    min_clean: 0.960, // Same floor — initial baseline pending first sweep
},
```

The `0.960` value is a copy of the Meshes.ba2 floor without any empirical measurement. The comment "initial baseline pending first sweep" has remained untouched. The actual MeshesExtra clean rate may be higher (making the gate loose and unable to catch regressions) or lower (making it fail unexpectedly).

`Fallout4 - MeshesExtra.ba2` exists at the local Steam install (`/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data/`).

## Fix

1. Run the ignored test to measure the actual MeshesExtra clean rate:
   ```sh
   cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored parse_rate_fo4_all_meshes
   ```
2. Record the measured clean rate.
3. Tighten `min_clean` to within 0.5% of the measured floor (round down).
4. Update the comment from "initial baseline pending first sweep" to reflect the measured value and date.

## Completeness Checks
- [ ] **SIBLING**: Check if any other `ArchiveSpec` entries in `parse_real_nifs.rs` have similar placeholder comments
- [ ] **TESTS**: The test itself is the deliverable — tightening the floor makes it the regression pin

_Filed from [docs/audits/AUDIT_FO4_2026-06-02.md](../blob/main/docs/audits/AUDIT_FO4_2026-06-02.md)_
