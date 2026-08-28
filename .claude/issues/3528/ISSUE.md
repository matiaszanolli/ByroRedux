# #3528 — SPT-2026-08-28-D3-01: every vanilla TREE.ICON is a bare filename, so the placeholder billboard's only visible surface never resolves

**Labels**: high, speedtree, terrain-exterior, import-pipeline, game:fnv, game:fo3, game:oblivion, bug
**Filed from**: `docs/audits/AUDIT_SPEEDTREE_2026-08-28.md` (`/audit-publish`, 2026-08-28)

---

**Severity**: HIGH
**Dimension**: TREE→Billboard Wiring (secondary: Per-Game Variants)
**Source**: `docs/audits/AUDIT_SPEEDTREE_2026-08-28.md` — SPT-2026-08-28-D3-01

**Location**:
- `byroredux/src/cell_loader/references/import.rs:318-320`
- `crates/spt/src/import/mod.rs:137-155`
- `byroredux/src/asset_provider/archive.rs:274-300` (`normalize_texture_path`)
- `byroredux/src/asset_provider/texture.rs:31-38`

## Description

The S1 deliverable is "a yaw-billboard quad **textured with the leaf texture**"
(`crates/spt/src/import/mod.rs:16-27`). The leaf texture is taken from the TREE record's
`ICON` sub-record, which wins over the `.spt`'s own tag 4003 (the tag-4003 path is
additionally rejected for vanilla content, since those are absolute exporter paths —
`is_relative_texture_path`, `import/mod.rs:352-359`). `ICON` is passed through verbatim:

```rust
// byroredux/src/cell_loader/references/import.rs:318
let leaf_texture_override = tree_record
    .map(|t| t.leaf_texture.as_str())
    .filter(|s| !s.is_empty());
```

and lands unmodified in `MaterialTextureSet::base_color` (`crates/spt/src/import/mod.rs:137-155`).
The archive lookup then applies the engine's only path normalisation:

```rust
// byroredux/src/asset_provider/archive.rs:289-299
let has_prefix = bytes.len() >= 9
    && bytes[..8].eq_ignore_ascii_case(b"textures")
    && (bytes[8] == b'\\' || bytes[8] == b'/');
if has_prefix { after_data } else { Cow::Owned(format!("textures\\{}", after_data)) }
```

**Every vanilla `TREE.ICON` is a bare filename with no directory component**, so this
produces `textures\<Name>.dds` — a path that does not exist in any shipped archive.
`resolve_texture_view_with_clamp` (`byroredux/src/asset_provider/texture.rs:346-420`) has no
alternate-path search: one `tex_provider.extract` miss and the material gets the magenta
checker handle.

## Evidence

- **ICON census** (run for the audit, over `FalloutNV.esm`, `Fallout3.esm`, `Oblivion.esm`):
  90 unique `TREE.ICON` values, **0 of which contain a path separator**. Per-game counts are
  3 / 9 / 81 — the FNV and FO3 numbers match `crates/plugin/tests/parse_real_esm.rs:843-859`
  and `:1395-1416`'s own "vanilla FNV ships 3 TREE bases" / "vanilla FO3 ships 9" assertions
  exactly, so the census captures the TREE set the engine's own parser sees. Samples:
  `WhiteOakLeaves01.dds`, `EuonymusBush01.dds`, `WastelandShrub01Foliage.dds` (FNV);
  `ElmLeaves01.dds`, `SugarMapleLeaves01.dds` (FO3); `DShrubLeaves01.dds`,
  `ShrubBoxwoodLeaves.dds`, `MTreeLeaves01.dds` (Oblivion).
- **Where those files actually live** (direct BSA folder-record + file-record walk):

  | ICON value | Real archive path |
  |---|---|
  | `WhiteOakLeaves01.dds` | `textures\trees\leaves\whiteoakleaves01.dds` (`Fallout - Textures2.bsa`) |
  | `WastelandShrub01Foliage.dds` | `textures\trees\leaves\wastelandshrub01foliage.dds` |
  | `EuonymusBush01.dds` | `textures\trees\leaves\euonymusbush01.dds` **and** `textures\trees\billboards\euonymusbush01.dds` |
  | `ShrubBoxwoodLeaves.dds` | `textures\trees\leaves\shrubboxwoodleaves.dds` (`Oblivion - Textures - Compressed.bsa`) |

- What the engine asks for instead: `textures\WhiteOakLeaves01.dds` — no such folder record
  exists in either archive.
- No compensating logic anywhere: `grep -rn "trees"` across `byroredux/src/asset_provider/`,
  `references/import.rs` and `crates/spt/src/import/mod.rs` returns only prose comments, no
  path construction.

## Impact

**100 % of vanilla SpeedTree placeholders on all three supported `.spt` games render with the
missing-texture checker instead of their leaf card.** This is the one thing the S1 placeholder
exists to do; the geometry, sizing (#1001/#1002), Z-up→Y-up bounds (#995), `-Z` winding (#1000),
billboard-on-mesh wiring (#3076) and wind response (#3190-#3195) are all correct and all
invisible behind a checker quad. It also matches the project's documented "chrome / posterized
⇒ run `tex.missing` first" symptom, which means any exterior-tree visual complaint filed
against lighting or the walker is likely to be this instead. No crash, no data loss, no GPU
hazard — visual only, but total and systematic.

## Related

- #1819 / SPT-NEW-05 — the *classifier* keyword collision on the same ICON strings, a
  different defect on the same field. That finding's own evidence quotes `ShrubBoxwoodLeaves.dds`
  and `WhiteOakLeaves01.dds` as bare filenames without noticing they never resolve.
- #468 — the original `textures\` prefix fix in `normalize_texture_path`, the same shape of bug
  one directory level up.
- #997 — the ICON-wins-over-tag-4003 precedence this defeats.

## Suggested Fix

**This finding deliberately does not propose a resolution rule.** Per the project's no-guessing
policy, the Bethesda `TREE.ICON` texture-resolution rule must be settled first — **do not
hardcode a prefix from this report's sample.** The corpus shows `textures\trees\leaves\` for
every sampled ICON, but `EuonymusBush01.dds` also exists under `textures\trees\billboards\`, so
a single blind prefix is not obviously the rule Bethesda's SpeedTree runtime used. Settle it
against the GECK/UESP `TREE:ICON` field documentation, then encode the resolved rule *once*.

The mechanically safe interim shape, which needs no format claim, is a candidate chain in the
SpeedTree route only (never in `normalize_texture_path`, which is shared by every other
consumer): probe `TextureProvider::has_texture` for the normalised path first, then a small
ordered list of `textures\trees\…` candidates, and log a single warning naming the ICON when
none hit. Pair it with a corpus test asserting all 90 vanilla ICON values resolve to a real
archive entry — that test is the actual regression guard, and it is cheap now that the census
exists.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other `.spt` route in `byroredux/src/scene/nif_loader.rs`, and any other bare-filename texture field)
- [ ] **CANONICAL-BOUNDARY**: The fix must stay in the SpeedTree route — per-game path logic never pushed into `normalize_texture_path` (shared by every consumer), into shaders/renderer, or re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix (the 90-value vanilla ICON corpus resolution test)
