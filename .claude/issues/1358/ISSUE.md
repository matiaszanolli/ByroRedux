# D8-08 / D8-NEW-01: BGEM base_color/base_color_scale not forwarded to ImportedMesh

**Severity**: HIGH (escalated from LOW per AUDIT_FO4_2026-06-02 D8-NEW-01)  
**Location**: `byroredux/src/asset_provider.rs` — BGEM branch of `merge_bgsm_into_mesh`

BGEM merge sets `mesh.emissive_color = bgem.emittance_color` (v≥11 additive glow) but never sets `base_color`/`base_color_scale`/`emissive_source`. All FO4 effect materials (fire, electricity, plasma, neon) render white instead of authored colour. Fix: set `emissive_color = base_color`, `emissive_mult = base_color_scale`, `emissive_source = Effect`. Also fix misleading "spec-glossiness convention" comment (D8-NEW-05).
