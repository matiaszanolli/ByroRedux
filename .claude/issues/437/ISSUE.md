# NIF-D2-C1: Introduce GameVariant enum at header parse — replace scattered bs_version thresholds

**Issue**: #437 — https://github.com/matiaszanolli/ByroRedux/issues/437
**Labels**: bug, nif-parser, high, legacy-compat

---

## Finding

NIF parser feature predicates key off raw `version` + single `bs_version` thresholds rather than a `GameVariant` enum. Version 20.2.0.7 is shared by Oblivion (final patch), FO3, FNV, and Skyrim LE — `user_version` / `bs_version` disambiguate, but scattered inline comparisons make the invariant hard to maintain.

Examples of the anti-pattern:
- `crates/nif/src/version.rs` — predicates like `has_bs_num_uv_sets()`, `uses_bs_lighting_shader()` key off raw thresholds
- `crates/nif/src/blocks/shader.rs`, `tri_shape.rs`, `skin.rs` — inline magic numbers `bs_version >= 34`, `>= 83`, `>= 100`, `>= 130`, `>= 155`

These correspond to FO3/FNV, Skyrim LE, Skyrim SE, FO4, FO76 but are not named.

## Impact

- **Reader can't tell which game a branch targets** — bugs like #149 (bool-gate regression) are easy to reintroduce.
- **Mis-parses for shared-version games**: e.g., FO3 vs FNV shader property tail bytes at identical 20.2.0.7 / bs_version=34 are not disambiguated.
- **Blocks non-Bethesda Gamebryo support**: the fallback "treat as Bethesda 20.2.0.7" coerces Civ4/Empire Earth/Divinity 2 content into the wrong parse path.
- **Fragile thresholds**: half-float gating at `bs_version >= 130` catches FO76 (155) and Starfield (172) by accident of monotonic versions.

## Fix

Introduce a `GameVariant` enum computed once at header parse:

```rust
// crates/nif/src/version.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameVariant {
    Oblivion,
    Fallout3,
    FalloutNV,
    SkyrimLE,
    SkyrimSE,
    Fallout4,
    Fallout76,
    Starfield,
    StockGamebryo,  // non-Bethesda (Civ4, Empire Earth, Divinity 2)
    Unknown,
}

impl GameVariant {
    pub fn detect(version: NifVersion, user_version: u32, bs_version: u32) -> Self {
        match (version.packed(), user_version, bs_version) {
            (0x14000005, _, _) if bs_version >= 3 && bs_version <= 11 => Self::Oblivion,
            (0x14020007, 11, 34) => Self::Fallout3,  // FO3 HEDR pattern
            (0x14020007, 11, 34 | 35) => Self::FalloutNV,
            (0x14020007, 12, 83) => Self::SkyrimLE,
            (0x14020007, 12, 100) => Self::SkyrimSE,
            (_, _, 130..=139) => Self::Fallout4,  // + Next-Gen patch
            (_, _, 155) => Self::Fallout76,
            (_, _, v) if v >= 168 => Self::Starfield,
            // ... etc.
            _ => Self::StockGamebryo,
        }
    }

    pub fn is_at_least(self, other: Self) -> bool { ... }
    pub fn uses_bslighting_shader(self) -> bool { matches!(self, Self::SkyrimLE | Self::SkyrimSE | Self::Fallout4 | Self::Fallout76 | Self::Starfield) }
    pub fn uses_half_float_verts(self) -> bool { matches!(self, Self::Fallout4 | Self::Fallout76 | Self::Starfield) }
    // etc.
}
```

Then re-express `bs_version >= N` thresholds as `variant.is_at_least(Fo4)` across the block parsers.

## Subsumes

Fixing this subsumes several Dim 2 findings:
- **NIF-D2-C2** (Bethesda-only allowlist accepts unknown bs_version → wrong path)
- **NIF-D2-H2** (bs_version step-function without named constants)
- **NIF-D2-M1** (half-float gating version-based not variant-based)
- **NIF-D2-M3** (user_version2/bs_version naming inconsistency)

Does NOT subsume:
- **NIF-D2-H1** (BSStreamHeader threshold duplicated — a separate helper method consolidation)
- **NIF-D2-H3** (endian byte never checked — independent bug)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Every `bs_version >= N` comparison in `crates/nif/src/` must migrate. Grep for `bsver()` / `bs_version` to enumerate.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Table-driven test covering each of the 8+ supported games' canonical `(version, user_version, bs_version)` triples. Every variant predicate (`uses_bslighting_shader`, `uses_half_float_verts`, `has_compact_material`, etc.) asserted at each point in the version matrix.

## Related

Aligns with `format_abstraction.md` memory note: "GameVariant trait pattern: per-game impls, not scattered version checks; applies to NIF/BSA/ESM."

## Source

Audit: `docs/audits/AUDIT_NIF_2026-04-18.md`, Dim 2 C1.
