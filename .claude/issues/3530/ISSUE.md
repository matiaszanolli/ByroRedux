# #3530 — LC-2026-08-27-D7-01: Oblivion's only authored parallax signal — NiTexturingProperty Apply Mode APPLY_HILIGHT2 — is read and discarded in the parser, and no other Oblivion parallax path exists

Labels: medium, bug, legacy-compat, nif-parser, nif, nifal, game:oblivion
Source: docs/audits/AUDIT_LEGACY_COMPAT_2026-08-27.md (base 969d81c8)
Filed: 2026-08-27 via /audit-publish

---

**From:** `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-27.md` (LC-2026-08-27-D7-01) · base `969d81c8`

- **Severity**: MEDIUM
- **Dimension**: 7 — property → pipeline mapping ("flag any property whose authored effect is dropped")
- **Location**: `crates/nif/src/blocks/properties.rs:205-209` (the discard), `:117` (`NiTexturingProperty.flags`, zero consumers), `crates/nif/src/import/material/legacy_properties.rs:210-238` + `crates/nif/src/blocks/properties.rs:262` (the version gate that makes the only implemented parallax path unreachable on Oblivion)

## Description

`NiTexturingProperty::parse` reads the Apply Mode field and throws it away without storing it, without a landing site, and — unlike `NiFogProperty` — **without any comment recording that the drop is deliberate**:

```rust
// Apply Mode: since 3.3.0.13, until 20.1.0.1.
// `until=` is inclusive per the version.rs doctrine — present at v20.1.0.1.
if stream.version() <= NifVersion::STRING_TABLE_THRESHOLD {
    let _apply_mode = stream.read_u32_le()?;
}
```

For NIF >= 20.1.0.2 the same field moves into the `Flags` bitfield (nif.xml:1585 — `TexturingFlags`, `width="3" pos="1" mask="0x000E" name="Apply Mode" type="ApplyMode"`; nif.xml:5232 gates `Flags` at `since="20.1.0.2"`). Redux stores that word (`properties.rs:117`, `pub flags: u16`) but **nothing in the workspace ever reads `NiTexturingProperty.flags`** — a grep for `tex_prop.flags` returns zero hits; the only `.flags` reads in the legacy walker are `TexDesc.flags` for clamp mode (`legacy_properties.rs:274`).

That drop is harmless on every post-Oblivion title (measured below), but on Oblivion it removes the game's only parallax signal. nif.xml annotates the value directly:

> `<option value="4" name="APPLY_HILIGHT2">Parallax Flag in some Oblivion meshes.</option>` (nif.xml:380)

and Gamebryo v3.2 confirms the value has no surviving general meaning — `/mnt/data/src/reference/gamebryo-v32/Include/NiTexturingProperty.h:72-80` renames modes 3 and 4 to `APPLY_DEPRECATED` / `APPLY_DEPRECATED2`.

The engine's three `parallax_map` producers are all **version-gated above Oblivion's v20.0.0.5**, so none can fire for it:

| Producer | Gate |
|---|---|
| `NiTexturingProperty` slot 7 (`legacy_properties.rs:215-217`) | `is_v20_2_0_5_plus` (`properties.rs:262`) — the slot does not exist on Oblivion |
| `BSShaderTextureSet` slot 3 (`legacy_properties.rs:339-342`) | `BSShaderPPLightingProperty`, FO3+ |
| `TextureRole::Height` (`dedicated_shader.rs:207`) | BGSM/BGEM, FO4+ |

So Oblivion parallax is currently 0% mapped, and the file-level flag that would enable it is destroyed one line after it is read.

## Evidence

Measured with a temporary counter on the discard site, driven over the vanilla archives (probe removed; tree unmodified).

- `Oblivion - Meshes.bsa` + all 7 DLC/SI mesh archives — 9,537 NIFs, all parsed: **35,161** `NiTexturingProperty` Apply Modes — 0 `APPLY_REPLACE`, 18 `APPLY_DECAL`, 32,810 `APPLY_MODULATE` (the no-op default), 900 `APPLY_HILIGHT`, **1,433 `APPLY_HILIGHT2`**.
- Base + Shivering Isles alone: **741 distinct NIFs of 9,470** carry at least one `APPLY_HILIGHT2`. Sampled paths are exactly the content class the convention is known for — `meshes\dungeons\caves\crmcornerinside01a.nif`, `meshes\dungeons\caves\crmfloorcrevice01a.nif`, `meshes\architecture\stonewall\stonewallbend02lm.nif`, `meshes\rocks\greatforest\lichen\rockgreatforest2080fgllichen.nif`.
- The post-Oblivion generation is unaffected: reading the Apply Mode bits out of `NiTexturingProperty.flags` over `Fallout - Meshes.bsa` (FNV, 14,881 NIFs), `Fallout - Meshes.bsa` (FO3) and `Skyrim - Meshes0.bsa` (29,851 NIFs combined) gives **2,258 + 996 properties, 100% `APPLY_MODULATE`, 0 NIFs with a non-default mode**. The gap is Oblivion-only and bounded.
- Height source: `Oblivion - Textures - Compressed.bsa` contains **zero** `_p.dds` entries, so Oblivion does not ship a separate parallax/height texture — consistent with the normal-map-alpha convention, for which the engine already has the analogous machinery on the Skyrim spec side (`NORMAL_ALPHA_SPEC_BIT`, `material_translate.rs:719`).

## Impact

Every parallax-authored Oblivion surface renders flat — 741 vanilla meshes, concentrated in cave and stone architecture and rock clutter, i.e. the interior/exterior surfaces a player looks at most. It is not a crash or a content loss (geometry and normal maps still render), which is why this is MEDIUM rather than HIGH under the "escalate if it removes visible game content" rule. The second-order cost is that the drop is invisible: with no field on the struct and no comment, nothing in the tree records that Oblivion parallax authoring was ever seen and declined, so this gap is not discoverable from the code.

## Related

No existing issue mentions `apply_mode`, `APPLY_HILIGHT`, or Oblivion parallax. Adjacent but distinct: **#3073 (OPEN)** — `parallax_height_scale` / `parallax_max_passes` bypassing the canonical `Material` — which is about the scalars, not about whether parallax is detected at all. Sibling convention already implemented: `resolve_normal_alpha_spec_roughness` (#1480). Nearby but different field: **#3517** / **#3516** (`TexDesc` clamp mode).

## Suggested Fix

Two steps, separable. (1) Stop discarding the field: store `apply_mode: u32` on `NiTexturingProperty` (decoding it from `(flags >> 1) & 0x7` on the >= 20.1.0.2 branch so both generations land in one place), which alone makes the authored value visible to a future consumer and to `mat.dump`. (2) Route `APPLY_HILIGHT2` into `MaterialInfo.parallax_map` on the Oblivion branch, sourcing height from the normal map's alpha via the same "bit tells the shader to sample another slot's alpha" mechanism `NORMAL_ALPHA_SPEC_BIT` already uses — do **not** add a `_p.dds` path-synthesis fallback, since the census shows no such texture ships. If step 2 is deferred, step 1 plus a one-line deliberate-skip comment in the `NiFogProperty` style is the minimum, so the next auditor does not have to re-derive this from nif.xml.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other legacy property parsers — `NiTexturingProperty`'s bump-map `luma_scale` / `luma_offset` / 2x2 matrix at `properties.rs:248-256` are dropped the same way)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
