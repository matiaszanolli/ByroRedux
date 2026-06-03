# FO4-D5-INFO-02: parse_rate_fo4_all_meshes MeshesExtra clean-rate floor is uncalibrated placeholder

**Severity**: LOW  
**Source**: AUDIT_FO4_2026-06-02 (D5-INFO-02)  
**Location**: `crates/nif/tests/parse_real_nifs.rs:219-222`

`min_clean: 0.960` for `Fallout4 - MeshesExtra.ba2` is a copy of the Meshes.ba2 floor with no empirical measurement. Comment: "initial baseline pending first sweep."

**Fix**: Run the ignored test against MeshesExtra, record the clean rate, tighten `min_clean` to within 0.5% of measured floor.
