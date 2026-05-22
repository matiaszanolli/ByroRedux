**Severity**: LOW
**Dimension**: Property → Material Mapping
**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-05-19.md` D4-NEW-02

`NiFogProperty` is parsed by the dispatch at `crates/nif/src/blocks/mod.rs:483` but the material walker at `crates/nif/src/import/material/walker.rs` has no `scene.get_as::<NiFogProperty>(idx)` arm. The parsed `fog_depth` / `fog_color` / `flags` triplet is silently dropped.

The 2026-04-30 audit listed NiFog under D4-NEW-01 as "wired into material pipeline (#558 / #607)" but that closure covered the per-node generic fog enable bit, not the NiFogProperty per-node fog override.

### Evidence
- `grep -n "NiFogProperty" crates/nif/src/import/material/walker.rs`: zero matches.
- The property's own docstring (`crates/nif/src/blocks/properties.rs:475`) notes "1 FO3 block observed in the wild" — extremely rare.

### Impact
In production: negligible. Per the docstring, 1 NiFogProperty exists across the entire vanilla FO3 corpus. Modded content could carry more.

### Suggested Fix
Either accept the gap and update the prior audit's claim, or add a minimal walker arm that surfaces `(fog_depth, fog_color)` onto the Material component's existing `fog_far_color` / `fog_far` fields when no cell-lighting fog is authored.

### Completeness Checks
- [ ] **TESTS**: Unit test on an FO3 NIF carrying NiFogProperty (rare; synthesise a fixture).
