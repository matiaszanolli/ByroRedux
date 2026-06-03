# FO4-D8-NEW-05: BGEM merge branch has misleading spec-glossiness comment

**Severity**: LOW  
**Source**: AUDIT_FO4_2026-06-02 (D8-NEW-05)  
**Location**: `byroredux/src/asset_provider.rs` — BGEM branch of `merge_bgsm_into_mesh`

Comment says "BGEM also uses the spec-glossiness convention" — incorrect. `BgemFile` has no `smoothness`/`specular_color`/`specular_mult` fields. The code path (NaN → keyword classifier) is correct; only the comment is wrong.

**Fix**: Replace comment with accurate description of the NaN-sentinel → keyword-classify path.
