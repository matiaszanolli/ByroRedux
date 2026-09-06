# #3916: FNV-2026-09-05-D8-02: `Update.bsa` is FNV's cross-kind patch archive, but the `fnv` profile lists it only under `default_bsas`, so its patched textures and sound are shadowed by the base archives

Filed from `docs/audits/AUDIT_FNV_2026-09-05.md` (FNV-2026-09-05-D8-02) via `/audit-publish`, 2026-09-05 (`/audit-suite --preset per-game-all`). Labels: `low,game:fnv,legacy-compat,import-pipeline,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3916 --json state`.

---

**Source**: `docs/audits/AUDIT_FNV_2026-09-05.md` (FNV-2026-09-05-D8-02), `/audit-suite --preset per-game-all`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: 8 — Real-Data Validation
- **Location**: `assets/debug_profiles.toml` — `[profiles.fnv]`
  `default_bsas` / `default_textures_bsas` / `default_sounds_bsas`;
  `byroredux/src/asset_provider/texture.rs` — `TextureProvider`'s separate
  `texture_archives` / `mesh_archives` pools
- **Status**: NEW (follow-on to closed #3790 / #3896, which got the *mesh* half
  right)
- **Description**: #3790 added `Update.bsa` and #3896 moved it to last position
  once #3637 inverted lookup precedence to last-wins. Both were about meshes.
  `Update.bsa` actually carries **four** asset kinds: 55 `.nif`, 25 `.kf`,
  **2 `.dds`**, **3 `.wav`**, 1 `.txt`. `TextureProvider` keeps
  `mesh_archives` and `texture_archives` as disjoint pools and
  `SoundArchiveProvider` is a third, so the two patched textures and the one
  patched sound are unreachable through the `--bsa` listing:

  | Update.bsa entry | shadowed by |
  |---|---|
  | `textures\dungeons\metro\tunnel\mettunpillar01.dds` | `Fallout - Textures2.bsa` (same key) |
  | `textures\terminals\nv_slotmachine\nv_slotmachine-minigame05.dds` | `Fallout - Textures2.bsa` (same key) |
  | `sound\fx\wpn\minigun\wpn_minigun_spin_lpm.wav` | `Fallout - Sound.bsa` (same key) |

  The other two `.wav` (`wpn_chainsaw_loop_lpm`, `obj_enclavecomtower`) and the
  `.txt` exist only in `Update.bsa` and are simply unreachable rather than
  shadowed.
- **Evidence**: full BSA index of `Update.bsa` (86 entries) intersected against
  `Fallout - Textures.bsa`, `Fallout - Textures2.bsa` and `Fallout - Sound.bsa`.
  Mesh half verified correct: 36 of the 55 `.nif` and 9 of the 25 `.kf` shadow
  `Fallout - Meshes.bsa`, and last-wins ordering makes `Update.bsa` win — the
  #3896 fix behaves as intended.
- **Impact**: Three patched assets out of 86 render/play in their unpatched
  form. Cosmetic. Recorded because the *shape* generalises: any archive listed
  in one pool patches only that pool, and `Update.bsa` is the only FNV archive
  that spans kinds.
- **Related**: #3790, #3896, #3637, #3788.
- **Suggested Fix**: Also list `Update.bsa` (last) in the `fnv` profile's
  `default_textures_bsas` and `default_sounds_bsas`. Opening the same 86-entry
  archive three times costs one extra directory `HashMap` each — the
  `open_with_numeric_siblings` dedup (`opened_paths`) is per-pool, so this is
  intended, not a double-open bug.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other block parsers, other games)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `translate_material` / `Material::resolve_pbr` / the emitter params, per-game logic stays at the NIFAL parser→`Material` boundary
