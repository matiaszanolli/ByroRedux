# #1350 — D3-04: SkinTint/HairTint shader types missing explicit texture slot-map arms

_Snapshot from AUDIT_FO4_2026-05-30. GitHub is authoritative for live state._

**Severity**: MEDIUM · **Source**: AUDIT_FO4_2026-05-30 (D3-04) · **Domain**: nif-parser / legacy-compat

**Location**: `crates/nif/src/import/material/walker.rs` (~line 233 — default `_` arm of `match shader.shader_type`)

**Description**: `BSLightingShaderType` values 5 (SkinTint) and 6 (HairTint) fall into the `_` default arm of the slot-map match. That arm routes texture set slot 4 → `env_map` and slot 5 → `env_mask`. Per nif.xml, SkinTint and HairTint declare no TS texture slots 4 or 5. Vanilla FO4 content leaves those slots empty so the `.filter(|s| !s.is_empty())` guard saves it — but a modded NIF with a non-empty slot 4 on a SkinTint mesh (realistic for mod-authored FO4 content) would erroneously import that path as an env map, producing incorrect metallic/green tinting.

**Evidence**: `walker.rs:239` comment reads "Glow 2, Parallax 3, SkinTint 5, HairTint 6, EnvironmentMap 1" — all in the default arm. The dedicated arms only cover FaceTint (4) and MultiLayerParallax (11).

**Impact**: Vanilla FO4 content is unaffected (empty-slot guard). Modded or future NIF content with a non-empty slot 4/5 on SkinTint/HairTint meshes gets an env map it shouldn't have.

**Suggested Fix**: Add explicit match arms for types 5 and 6 that skip slots 4 and 5:
```rust
5 | 6 => { /* SkinTint / HairTint — no texture slots; drive from tint color only */ }
```

## Completeness Checks
- [ ] **SIBLING**: Check whether shader types beyond 11 that aren't in dedicated arms could have similar slot misrouting (type 12 Sparkle Snow, type 14 Eye, etc.)
- [ ] **TESTS**: Add a test building a SkinTint property with a non-empty slot 4 and asserting `env_map` is NOT set
