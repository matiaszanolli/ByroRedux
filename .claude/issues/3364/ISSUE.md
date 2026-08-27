# SKY-2026-08-27-D2-01: `canonical_shader_type` translates `BSShaderType155` for the FO76 layout only, but the parser feeds Starfield the same 155 enum

Labels: low,nif-parser,nif,bug,game:starfield,legacy-compat

- **Severity**: LOW
- **Confidence**: PLAUSIBLE (code-read only — no Starfield install on this machine to census)
- **Location**: `crates/nif/src/import/material/slot_role.rs:140-153` (`canonical_shader_type`),
  paired with `crates/nif/src/blocks/shader.rs:878-880` and
  `crates/nif/src/blocks/shader.rs:1582-1614` (`parse_shader_type_data_fo76`)
- **Description**: The parser dispatches `bsver >= FO76 (155)` — which *includes* Starfield
  (`bsver >= 172`) — to `parse_fo76_plus`, whose `shader_type` is a `BSShaderType155` value
  decoded by `parse_shader_type_data_fo76`. `parse_fo76_plus`'s own doc states this
  explicitly: *"Starfield (BSVER 172+) reuses the FO76 enum."* But the importer's
  enum-translation step only remaps when the layout is `Fallout76`:

  ```rust
  pub const fn canonical_shader_type(layout: TextureSlotLayout, raw: u32) -> u32 {
      if matches!(layout, TextureSlotLayout::Fallout76) {
          match raw {
              3 => bs_lighting::FACE_TINT,   // 4
              4 => bs_lighting::SKIN_TINT,   // 5
              5 => bs_lighting::HAIR_TINT,   // 6
              12 => bs_lighting::EYE_ENVMAP, // 16
              17 => 0,
              _ => raw,
          }
      } else { raw }
  }
  ```

  `TextureSlotLayout::from_bsver` returns `Starfield` (not `Fallout76`) for `bsver >= 172`
  (`slot_role.rs:105-113`), so Starfield raw types fall through untranslated and are then
  consumed as Skyrim `BSLightingShaderType` numbers by `slot_to_role` and by
  `info.material_kind`. `normalize_shader_type` (`dedicated_shader.rs:47-57`) masks two of
  the five divergences because the *payload* variants `Fo76SkinTint` (155 type 4) and
  `HairTint` (155 type 5) carry the tag; the three that parse to `ShaderTypeData::None`
  do not:

  | 155 raw | means | reaches slot table as | Skyrim meaning |
  |---|---|---|---|
  | 3 | Face Tint | 3 | Parallax |
  | 12 | Eye Envmap | 12 | Tree Anim |
  | 17 | Terrain | 17 | Cloud (FO76 arm degrades this to 0) |

  Consequence for a Starfield type-3 (FaceTint) property with a `BSShaderTextureSet`:
  `slot_to_role((Starfield, 3))` hits `(Skyrim | Starfield, 3) => match shader_type {
  FACE_TINT => Detail, _ => Height }` (`slot_role.rs:273-277`) and binds the head's detail
  map as a POM height field, while slot 2's `_sk` tint mask is dropped because
  `tint_family` is false — the exact failure #2694 fixed for Skyrim FaceTint.
- **Evidence**:
  - nif.xml enum split, `/mnt/data/src/reference/nifxml/nif.xml:1400` and `:1425`:
    `BSLightingShaderType` `4 = Face Tint / 5 = Skin Tint / 6 = Hair Tint / 16 = Eye Envmap`
    vs `BSShaderType155` `3 = Face Tint / 4 = Skin Tint / 5 = Hair Tint / 12 = Eye Envmap /
    17 = Terrain`.
  - `shader.rs` parse gate:
    ```rust
    let mut me = if bsver >= crate::version::bsver::FO76 {
        Self::parse_fo76_plus(stream, bsver)?
    ```
    with `FO76 = 155`, `STARFIELD = 172` (`crates/nif/src/version.rs:448,453`) — so Starfield
    takes the FO76 arm and `parse_shader_type_data_fo76`.
  - `slot_role.rs` layout gate: `if bsver >= STARFIELD { Self::Starfield } else if bsver >= FO76 { Self::Fallout76 }`.
  - Not verifiable against data here: the SteamLibrary root holds Skyrim SE, Oblivion,
    FO3 GOTY, FNV and FO4 — no Starfield install, so the instance count is **unsourced**.
- **Impact**: Starfield full-body `BSLightingShaderProperty` blocks (the ones with an empty
  `net.name`, which bypass the material-reference stub) with shader type 3/12/17 get the
  wrong canonical material kind and, if they bind a `BSShaderTextureSet`, the wrong texture
  roles. Zero impact on Skyrim, FO4, FO76 or the legacy games. The module doc at
  `slot_role.rs:17-23` asserts Starfield materials "deliberately do not enter this table"
  (their roles come from the CDB) — if that holds universally the slot half is inert, but
  the table nonetheless carries explicit `TextureSlotLayout::Starfield` arms and
  `info.shader_type` / `info.material_kind` are written regardless of whether a texture set
  exists.
- **Suggested Fix**: change the guard to
  `if matches!(layout, TextureSlotLayout::Fallout76 | TextureSlotLayout::Starfield)`, since
  the *parser* boundary (`bsver >= FO76`) is what decides which enum the integer came from —
  keep the translation keyed on the same boundary rather than on a narrower layout tag. Add
  a unit test pinning `canonical_shader_type(Starfield, 3) == FACE_TINT`, mirroring the
  existing FO76 pins in `shader_type_data_tests.rs:162-198`. Confirm against a Starfield
  corpus census before treating the routing half as user-visible.
- **Related**: #3085 (CLOSED — the FO76 sibling: slot-6 arm keyed on a Skyrim shader type
  FO76's enum cannot produce), #2579 (the FO76 remap that introduced
  `canonical_shader_type`), #2695 (single slot→role table), #2616 (established that
  Starfield reuses the FO76 wire layout). Checked the 300-issue dedup baseline (84 open, fetched 2026-08-27): no OPEN issue
  covers this.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27.md` (`/audit-skyrim`, 7 dimensions),
verified against HEAD `558af58c` on a full vanilla Skyrim SE install.*
