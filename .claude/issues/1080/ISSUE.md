# FO4-D3-009: Add BSVER-130 stopcond boundary test for BSLightingShaderProperty

**Source**: AUDIT_FO4_2026-05-15.md · LOW  
**Location**: `crates/nif/src/blocks/shader_tests.rs`  
FO4 (bsver=130) BGSM-named shaders must NOT set material_reference=true. No test pins this.
