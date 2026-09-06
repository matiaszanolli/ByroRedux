# #3913: FNV-2026-09-05-D8-01: all three water-splash candidate paths are Skyrim-shaped or mistyped, so M44 water acoustics is a guaranteed silent no-op on FNV *and* FO3, and the `#3788` profile comment's "canonical footstep/splash paths byte-for-byte" claim is half false

Filed from `docs/audits/AUDIT_FNV_2026-09-05.md` (FNV-2026-09-05-D8-01) via `/audit-publish`, 2026-09-05 (`/audit-suite --preset per-game-all`). Labels: `medium,game:fnv,legacy-compat,audio,import-pipeline,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3913 --json state`.

---

**Source**: `docs/audits/AUDIT_FNV_2026-09-05.md` (FNV-2026-09-05-D8-01), `/audit-suite --preset per-game-all`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: 8 — Real-Data Validation
- **Location**:
  - `byroredux/src/asset_provider/texture.rs` — `try_load_default_water_splash`'s `CANDIDATES` array
  - `byroredux/src/systems/audio.rs` — `water_audio_system`'s `config.splash_sound` early-return
  - `assets/debug_profiles.toml` — the `#3788` comment above `default_sounds_bsas` in `[profiles.fnv]`
- **Status**: NEW (sequel to closed #3788 / FNV-2026-08-30-D8-01; not covered by
  #3189, which is about the loader re-opening the archive, not about its paths)
- **Description**: `try_load_default_water_splash` tries three hardcoded
  archive keys in order and takes the first hit. Its doc says the list
  "cover[s] the Skyrim and Fallout naming variants". Measured against the real
  archives, **all three are Skyrim keys or typos** — none exists in any Fallout
  title:

  | Candidate | `Fallout - Sound.bsa` (FNV) | FO3 `Fallout - Sound.bsa` | `Skyrim - Sounds.bsa` |
  |---|---|---|---|
  | `sound\fx\phy\water\phy_water_m_01.wav` | **MISS** | **MISS** | HIT |
  | `sound\fx\phy\phy_water_m_01.wav` | **MISS** | **MISS** | HIT |
  | `sound\fx\fst\water\walk\l\fst_water_walk_03.wav` | **MISS** | **MISS** | — |

  The real paths differ by one path segment each:
  - FNV medium splash is `sound\fx\phy\water\`**`splash_m\`**`phy_water_m_01.wav`
    (siblings `splash_h\`, `splash_l\`, and `human\npc_human_splash_0{1,2,3}.wav`).
  - FO3's is `sound\fx\phy\water\`**`medium\`**`phy_water_m_01.wav`.
  - The third candidate's `walk\`**`l\`** should be `walk\`**`left\`**
    (`sound\fx\fst\water\walk\left\fst_water_walk_03.wav` **does** exist on FNV).

  So `WaterAudioConfig.splash_sound` is permanently `None` on FNV, and
  `water_audio_system` early-returns on every entry/exit. Not a regression: it
  has never worked on Fallout. It became *observable* only after #3788 gave
  `--game fnv` a `--sounds-bsa` at all — before that the loader bailed at
  `sounds.is_empty()` and the path was never reached.
- **Evidence**: independent BSA index of `Fallout - Sound.bsa` (6 465 entries),
  FO3's `Fallout - Sound.bsa`, and `Skyrim - Sounds.bsa` (6 198 entries). The
  first candidate resolves in the Skyrim archive and in neither Fallout archive;
  the third's `l\` segment resolves nowhere while `left\` resolves on FNV.
  `try_load_default_footstep`'s `CANONICAL`
  (`sound\fx\fst\dirt\walk\left\fst_dirt_walk_01.wav`) **does** hit — the
  footstep half of the profile comment is correct, the splash half is not.
  `grep splash_sound` confirms `try_load_default_water_splash` is its only
  writer, so there is no second path that could mask this.
- **Impact**: One shipped M44 subsystem (water acoustics, `948f104a`) produces
  no audio at all on the reference title and on FO3. It fails loudly enough to
  find (`log::warn!("water acoustics: no splash candidate found …")`) and
  quietly enough to have survived four consecutive `/audit-audio` cycles, all of
  which examined this function for *archive-open* hygiene and never checked
  whether its paths exist. Audio-only, non-crashing → MEDIUM, matching how last
  cycle scored the structurally identical FNV-2026-08-30-D1-01.
- **Related**: #3788 (closed) supplied the archive; #3189 (loader re-open
  hygiene); AUD-2026-08-20-D7-02.
- **Suggested Fix**: Add the two real Fallout keys to `CANDIDATES`
  (`sound\fx\phy\water\splash_m\phy_water_m_01.wav` for FNV,
  `sound\fx\phy\water\medium\phy_water_m_01.wav` for FO3) and correct the third
  candidate's `walk\l\` → `walk\left\`. Then fix the `#3788` comment in
  `debug_profiles.toml`, which currently asserts a byte-for-byte splash
  resolution that has never held. A test that asserts each candidate against a
  real archive would have caught this; a source-only test cannot.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other block parsers, other games)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `translate_material` / `Material::resolve_pbr` / the emitter params, per-game logic stays at the NIFAL parser→`Material` boundary
