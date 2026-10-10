use super::super::common::{read_lstring_or_zstring, read_zstring, remap_fid, CommonNamedFields};
use crate::esm::reader::{FormIdRemap, GameKind, SubRecord};
use crate::esm::sub_reader::SubReader;

#[derive(Debug, Clone, Default)]
pub struct RaceRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// Skill bonuses: `(skill_index, bonus)` pairs, with `0xFF` (None)
    /// slots dropped. Oblivion / FO3 / FNV author 8 pairs of the 0x0C..=0x20
    /// `SkillIndex` values; Skyrim authors 7 pairs of its own actor-value
    /// skill indices (6 = One-Handed … 23 = Enchanting).
    ///
    /// Always empty for FO4 / FO76 / Starfield: those games have no skills,
    /// and their RACE `DATA` carries no bonus array at all (#2455).
    ///
    /// Pre-#967 this field was typed as `Vec<(u32, i8)>` under the
    /// premise that the bonus was form-keyed by `AVIF` reference.
    /// OpenMW's `esm4/loadrace.cpp:135-153` (the canonical TES4 /
    /// FO3 / FONV reader) reads `(u8 skill_index, u8 bonus) × 8`
    /// from a 36-byte DATA — confirmed by the `subHdr.dataSize == 36`
    /// gate they ship against vanilla content. Our previous wider
    /// type was reading 5-byte strides through a 16-byte payload
    /// and surfacing garbage `form_id` values.
    pub skill_bonuses: Vec<(u8, i8)>,
    /// Body part model paths (head, body, hand, foot).
    pub body_models: Vec<String>,
    /// Skyrim RACE ANAM skeleton paths, indexed male then female. These
    /// belong to the opening MNAM/FNAM section, not the later MODL body
    /// or behavior-project sections. Empty means no authored path.
    pub skeleton_models: [String; 2],
    /// `INDX` + `MODL` head-part pairs from the RACE **head** section
    /// (the run opened by `NAM0` and closed by `NAM1`). Each entry is
    /// `(head_part_index, mesh_path, gender_section)`; the index is
    /// the game's own numbering, so resolve it through
    /// [`head_part::index_of`] rather than by literal value — Oblivion
    /// and FO3 / FNV disagree from `Mouth` down.
    ///
    /// `gender_section` tracks which RACE-record section the part
    /// was authored in:
    ///
    ///   * `None`  — shared across both genders (entries before any
    ///     MNAM/FNAM marker). Every Oblivion entry lands here: its
    ///     head section is one ungendered run, and the male/female
    ///     split lives in the *ear* index instead.
    ///   * `Some(0)` — male-only (after an `MNAM` section marker).
    ///   * `Some(1)` — female-only (after `FNAM`). FO3 / FNV author
    ///     the entire head section twice, once per gender — including
    ///     the head itself, so the female head is `Some(1)` at index
    ///     `Head`, not a `None` entry (#3418).
    ///
    /// Without this typed pairing the spawner can't tell which
    /// `body_models` entry is which body part — or which gender it
    /// applies to. Pre-fix every NPC rendered with just the head NIF —
    /// no eyes, mouth, teeth, tongue. Populated on Oblivion and
    /// FO3 / FNV; empty on Skyrim+, which moved head parts out to
    /// standalone `HDPT` records.
    ///
    /// The body section past `NAM1` restarts `INDX` at 0 for a
    /// different vocabulary and is deliberately **not** collected here
    /// (#3419) — it is still appended to [`Self::body_models`], which
    /// stays a flat append-ordered list of every `MODL` in the record
    /// and therefore also carries `.egt` texture paths.
    pub head_parts: Vec<(u32, String, Option<u8>)>,
    /// Head-section `ICON` texture per head part: `(INDX, path, gender
    /// section)`, the same shape as [`Self::head_parts`]. Each part's `ICON`
    /// follows its `MODL` under the same `INDX` (measured on FO3
    /// `Caucasian`: `INDX 0` → `MODL Characters\Head\HeadOld.NIF` →
    /// `ICON Characters\Old\HeadHuman.dds`; the female section's head
    /// names `Characters\OldFemale\HeadHuman.dds`), and an ear slot may
    /// author an `ICON` with no `MODL`. This is the race / gender base skin
    /// texture for the head — the head NIF's own texture is the male
    /// default, so without it female heads rendered with male skin.
    pub head_part_textures: Vec<(u32, String, Option<u8>)>,
    /// #5487 — body-section `ICON` skins, `(INDX, path, gender section)`.
    /// The section past `NAM1` restarts `INDX` at 0 for the body
    /// vocabulary: 0 UpperBody, 1 Leg, 2 Hand, 3 Foot, 4 Tail (xEdit
    /// `wbRaceBodyTextureIndex`). Oblivion authors per-race, per-gender
    /// skins here (`Characters\Argonian\Male\UpperBodyMale.dds`, …) while
    /// the body NIFs themselves all author the Imperial skin — pre-fix
    /// the section was dropped wholesale (the `ICON` arm gated on
    /// `in_head_section`), so every beast and mer torso rendered human.
    /// FO3 / FNV author body textures in the head-section table instead;
    /// this list is empty there and on Skyrim+.
    pub body_part_textures: Vec<(u32, String, Option<u8>)>,
    /// Default body height per gender, from DATA — `(male, female)`.
    /// Vanilla values run ~0.8..1.2 (Skyrim `NordRace` is 1.03; FO4
    /// `HumanChildRace` is 0.825). Decoded for Oblivion / FO3 / FNV
    /// (offset 16), Skyrim (offset 16 after its 7 skill pairs), and
    /// FO4 / FO76 (offset 0) — see `parse_race` for the three layouts.
    /// Stays `(1.0, 1.0)` when the game ships no RACE `DATA` at all
    /// (Starfield) or the sub-record is too short.
    pub base_height: (f32, f32),
    /// Default body weight per gender, from DATA — `(male, female)`.
    /// Vanilla values typically `(1.0, 1.0)`.
    ///
    /// Populated for Oblivion / FO3 / FNV and Skyrim. **Not** populated for
    /// FO4 / FO76: those encode a three-axis (thin / muscular / large) morph
    /// per gender that this two-field pair cannot represent, and the bytes
    /// are invariant across every shipped race so no reading of them can be
    /// validated against the data (#2455).
    pub base_weight: (f32, f32),
    /// `RACE_FLAGS` u32 from DATA (offset 32 on every decoded layout).
    /// Bit 0 = Playable. Left at `0` for FO4 / FO76, whose flag semantics
    /// are unverified — their non-playable `HumanChildRace` sets bit 0,
    /// contradicting the TES-lineage meaning (#2455).
    /// Other bits are documented per game (`BeastRace`, `Swims`,
    /// `Flies` — vanilla Oblivion uses bit 0 + bit 2 = 0x05 for
    /// playable beast-race overrides).
    pub race_flags: u32,
    /// TES5 RACE `DATA` starting Health (`f32 @ 36`). `None` for games whose
    /// layout has not been modelled and for malformed/non-finite values.
    pub starting_health: Option<f32>,
    /// TES5 RACE `DATA` starting Magicka (`f32 @ 40`).
    pub starting_magicka: Option<f32>,
    /// TES5 RACE `DATA` starting Stamina (`f32 @ 44`).
    pub starting_stamina: Option<f32>,
    /// Per-gender base attributes from the Oblivion-only `ATTR`
    /// sub-record. 8 attributes per gender (Strength / Intelligence
    /// / Willpower / Agility / Speed / Endurance / Personality /
    /// Luck) × 2 = 16 bytes total. `None` outside Oblivion.
    pub base_attributes: Option<RaceAttributes>,
    /// Default hair form IDs from the Oblivion `DNAM` sub-record —
    /// `(male, female)`. `None` when DNAM is absent (FO3 / FNV /
    /// Skyrim use a different default-hair mechanism).
    pub default_hair: Option<(u32, u32)>,
    /// Default voice form IDs from the Oblivion `VNAM` sub-record —
    /// `(male, female)`. `None` when VNAM is absent or has the
    /// TES5 4-byte shape.
    pub voice_forms: Option<(u32, u32)>,
    /// FaceGen main clamp from `PNAM` (1 × f32). Vanilla value is
    /// 5.0; `None` when the sub-record is absent.
    pub facegen_main_clamp: Option<f32>,
    /// FaceGen face clamp from `UNAM` (1 × f32). Vanilla value is
    /// 3.0; `None` when the sub-record is absent.
    pub facegen_face_clamp: Option<f32>,
    /// Race-vs-race disposition adjustments from repeated `XNAM`
    /// sub-records — each pair is `(other_race_form_id, adjustment)`.
    /// Drives the Radiant-AI faction-mood calculation for Oblivion
    /// NPCs interacting across racial lines.
    pub race_reactions: Vec<(u32, i32)>,
    /// #4415 — the race's spell list (`SPLO`): racial abilities and
    /// powers every member of the race carries. Oblivion / Skyrim / FO4;
    /// FO3/FNV RACE records author none.
    pub spells: Vec<u32>,
    /// Default "naked skin" ARMO form ID from the Skyrim+ `WNAM`
    /// sub-record — the race's implicit base-layer armor every actor
    /// wears beneath OTFT/CNTO gear. `None` outside `uses_prebaked_
    /// facegen()` games (Skyrim/FO4/FO76/Starfield) or when the RACE
    /// omits `WNAM` (see #2093 / SKY-D3-NEW-01: without this, a
    /// prebaked NPC whose equipped gear doesn't cover a biped region
    /// has zero mesh source for it — the FaceGeom NIF bakes head-only
    /// geometry, no body).
    pub default_skin: Option<u32>,
}

/// RACE `INDX` head-part identifiers, resolved by semantic role.
///
/// The raw `INDX` number is **not** portable across games: Oblivion
/// numbers a nine-entry table that carries a separate male and female
/// ear slot, while FO3 / FNV drop the second ear (their head section is
/// already split by `MNAM` / `FNAM`) and every role from Mouth down
/// shifts one lower. Both tables were dumped from vanilla content:
///
/// | role         | Oblivion | FO3 / FNV |
/// |--------------|----------|-----------|
/// | Head         | 0        | 0         |
/// | Ear (male)   | 1        | 1         |
/// | Ear (female) | 2        | 1         |
/// | Mouth        | 3        | 2         |
/// | Teeth lower  | 4        | 3         |
/// | Teeth upper  | 5        | 4         |
/// | Tongue       | 6        | 5         |
/// | Left eye     | 7        | 6         |
/// | Right eye    | 8        | 7         |
///
/// (`Oblivion.esm` `Imperial` `00000907` — `HeadHuman` 0, `EarsHuman`
/// 1 + 2, `MouthHuman` 3, `TeethLowerHuman` 4, `TeethUpperHuman` 5,
/// `TongueHuman` 6, `EyeLeftHuman` 7, `EyeRightHuman` 8;
/// `FalloutNV.esm` `CaucasianOldAged` `000987DF` — `HeadOld` 0, ear
/// ICON-only 1, `MouthHuman` 2, `TeethLowerHuman` 3,
/// `TeethUpperHuman` 4, `TongueHuman` 5, `EyeLeftHuman` 6,
/// `EyeRightHuman` 7.) UESP's `RACE_HeadPart` table documents the
/// Oblivion numbering only; the FO3 / FNV shift is why the spawner
/// must ask by [`Role`] rather than by literal index. Pre-#3420 the
/// spawner hard-coded 6 / 7 for the eyes, which on the Oblivion arm
/// selected the *tongue* and the *left eye* — and then painted the
/// NPC's eye texture over both.
pub mod head_part {
    use crate::esm::reader::GameKind;

    /// A head sub-mesh slot, independent of the per-game `INDX`
    /// numbering. [`index_of`] maps one to the number a given game's
    /// RACE record actually authors.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Role {
        Head,
        EarMale,
        EarFemale,
        Mouth,
        TeethLower,
        TeethUpper,
        Tongue,
        LeftEye,
        RightEye,
    }

    /// The head sub-parts the NPC spawner mounts on the skeleton
    /// beside the base head NIF. Ears are game-dependent (FNV models
    /// them into the head mesh and authors only an `ICON` at index 1;
    /// Oblivion ships a real `EarsHuman.nif`), so they are resolved
    /// per gender by the caller rather than listed here.
    pub const ORAL_ROLES: [Role; 4] = [
        Role::Mouth,
        Role::TeethLower,
        Role::TeethUpper,
        Role::Tongue,
    ];

    /// The `INDX` value a game authors for `role`, or `None` when the
    /// game's RACE record has no head-part table at all (Skyrim+ moved
    /// head parts out to standalone `HDPT` records).
    pub fn index_of(game: GameKind, role: Role) -> Option<u32> {
        match game {
            GameKind::Oblivion => Some(match role {
                Role::Head => 0,
                Role::EarMale => 1,
                Role::EarFemale => 2,
                Role::Mouth => 3,
                Role::TeethLower => 4,
                Role::TeethUpper => 5,
                Role::Tongue => 6,
                Role::LeftEye => 7,
                Role::RightEye => 8,
            }),
            GameKind::Fallout3NV => Some(match role {
                Role::Head => 0,
                // One ear slot for both genders — the gender split is
                // the MNAM / FNAM section, not the index.
                Role::EarMale | Role::EarFemale => 1,
                Role::Mouth => 2,
                Role::TeethLower => 3,
                Role::TeethUpper => 4,
                Role::Tongue => 5,
                Role::LeftEye => 6,
                Role::RightEye => 7,
            }),
            GameKind::Skyrim | GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield => {
                None
            }
        }
    }
}

/// Per-gender attribute block for `RaceRecord.base_attributes`
/// (Oblivion `ATTR` sub-record). 8 attributes × 2 genders = 16 bytes.
#[derive(Debug, Clone, Default)]
pub struct RaceAttributes {
    pub male: GenderedAttributes,
    pub female: GenderedAttributes,
}

#[derive(Debug, Clone, Default)]
pub struct GenderedAttributes {
    pub strength: u8,
    pub intelligence: u8,
    pub willpower: u8,
    pub agility: u8,
    pub speed: u8,
    pub endurance: u8,
    pub personality: u8,
    pub luck: u8,
}

/// #3714 — takes the load-order `remap` because `WNAM` (the default skin
/// ARMO) and Oblivion's `XNAM` race-reaction pairs are embedded FormIDs.
/// `WNAM` is looked up in `EsmIndex.items`, which is keyed in GLOBAL space,
/// so a raw id missed for every DLC- or ESL-added race and the armor
/// resolved to zero meshes — indistinguishable from an unshipped mesh.
pub fn parse_race(
    form_id: u32,
    subs: &[SubRecord],
    game: GameKind,
    remap: &Option<FormIdRemap>,
) -> RaceRecord {
    // Helper claims a single MODL; RACE records carry multiple body
    // parts in MODL — keep that arm custom and ignore the helper's
    // last-MODL string. TD3-203 / #1113.
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    let mut record = RaceRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        description: String::new(),
        skill_bonuses: Vec::new(),
        body_models: Vec::new(),
        skeleton_models: Default::default(),
        head_parts: Vec::new(),
        head_part_textures: Vec::new(),
        body_part_textures: Vec::new(),
        base_height: (1.0, 1.0),
        base_weight: (1.0, 1.0),
        race_flags: 0,
        starting_health: None,
        starting_magicka: None,
        starting_stamina: None,
        base_attributes: None,
        default_hair: None,
        voice_forms: None,
        facegen_main_clamp: None,
        facegen_face_clamp: None,
        race_reactions: Vec::new(),
        default_skin: None,
        spells: Vec::new(),
    };

    let is_oblivion = matches!(game, GameKind::Oblivion);

    // FNV / FO3 head-part pairing — each MODL is preceded by an INDX
    // that names which body part the model is (0 = Head, 7 = Left Eye,
    // 8 = Right Eye, …). Track the most-recent INDX so we can attach
    // an index to the next MODL we see. Reset on consumption so a
    // stray MODL with no INDX prefix isn't mis-labelled. See
    // `RaceRecord::head_parts`.
    //
    // Vanilla FO3 / FNV RACE records split per-gender body parts via
    // `MNAM` (Male marker) and `FNAM` (Female marker) sub-records,
    // after the shared INDX/MODL block. Track which section we're in
    // so the spawner can pick gender-appropriate models without
    // double-rendering male+female eyes on the same NPC.
    //
    // #3419: `INDX` is re-used — with its own 0..3 numbering and its
    // own `MNAM` / `FNAM` markers — by the *body* section that opens
    // at `NAM1`. Without a section gate the head-part map absorbs
    // `characters\_Male\UpperBody.nif` under `HEAD = 0`,
    // `RightHand.nif` under FNV's `MOUTH = 2` and an `.egt` texture
    // path under `TEETH_LOWER = 3` (dumped from `CaucasianOldAged`,
    // `000987DF`). `NAM0` opens the head section, `NAM1` closes it;
    // both markers also reset the gender + pending-INDX state, since
    // each section restarts its own MNAM/FNAM/INDX run. Records that
    // author neither marker (none in vanilla, but a mod could) keep
    // the pre-#3419 behaviour of treating everything as head data.
    let mut pending_indx: Option<u32> = None;
    // The head-part `INDX` a following `ICON` belongs to. Unlike
    // `pending_indx` it survives the part's `MODL`, which precedes the
    // `ICON`, and covers ICON-only slots (the FO3 / FNV ear).
    let mut icon_indx: Option<u32> = None;
    let mut gender_section: Option<u8> = None;
    let mut in_head_section = true;
    let mut in_skeleton_section = true;

    for sub in subs {
        match &sub.sub_type {
            b"DESC" => record.description = read_lstring_or_zstring(&sub.data),
            // #4415 — racial spell list, same one-FormID shape as NPC_.
            b"SPLO" if sub.data.len() >= 4 => {
                let raw = SubReader::new(&sub.data).u32_or_default();
                record.spells.push(remap_fid(raw, remap));
            }
            // DATA (TES4 / FO3 / FONV — 36 bytes total):
            //   8 × (u8 skill_index, u8 bonus)    16 B
            //   heightMale + heightFemale (2 × f32) 8 B
            //   weightMale + weightFemale (2 × f32) 8 B
            //   raceFlags (u32)                     4 B
            // Pre-#967 this arm read 7 × (u32, i8) starting at offset 0,
            // surfacing garbage form-keyed bonuses and dropping height/
            // weight/flags entirely. Layout per OpenMW
            // `esm4/loadrace.cpp:135-153` (canonical TES4-era reader).
            // Gate on the TES4/FO3/FNV era so a Skyrim 128/164-byte DATA
            // (which also satisfies `len >= 36`) is not mis-decoded with the
            // 36-byte layout into garbage skill bonuses / height / weight /
            // flags (#1629). Skyrim+ is handled by the two arms below.
            b"DATA"
                if matches!(game, GameKind::Oblivion | GameKind::Fallout3NV)
                    && sub.data.len() >= 36 =>
            {
                let mut r = SubReader::new(&sub.data);
                for _ in 0..8 {
                    let skill = r.u8_or_default();
                    let bonus = r.u8_or_default() as i8;
                    // Skip 0xFF (Skill_None sentinel) — OpenMW maps
                    // these into a HashMap keyed by SkillIndex, but
                    // our flat Vec preserves authoring order without
                    // dropping non-None slots. Skipping None keeps
                    // the public Vec semantically "real skill
                    // bonuses only," mirroring the FNV-era intent.
                    if skill != 0xFF {
                        record.skill_bonuses.push((skill, bonus));
                    }
                }
                let h_m = r.f32_or_default();
                let h_f = r.f32_or_default();
                let w_m = r.f32_or_default();
                let w_f = r.f32_or_default();
                record.base_height = (h_m, h_f);
                record.base_weight = (w_m, w_f);
                record.race_flags = r.u32_or_default();
            }
            // DATA (TES5 — Skyrim LE 128 B / SE 164 B):
            //   7 × (u8 skill_index, u8 bonus)      14 B
            //   u16 padding                          2 B
            //   heightMale + heightFemale (2 × f32)  8 B
            //   weightMale + weightFemale (2 × f32)  8 B
            //   raceFlags (u32)                      4 B
            //   … then 92 / 128 B of TES5-only tail (starting health /
            //     magicka / stamina, carry weight, accel + decel rates,
            //     regen rates, unarmed damage and reach, biped object slots)
            //     of which the combat actor-value path currently consumes
            //     starting health, magicka, and stamina.
            //
            // Layout per OpenMW `esm4/loadrace.cpp:154-170`, and verified
            // byte-for-byte against vanilla `Skyrim.esm` (2026-08-12), which
            // ships 164-byte DATA for all 99 of its RACE records:
            //   * `NordRace` decodes to skill indices 7/6/9/10/17/12 with
            //     bonuses +10/+5/+5/+5/+5/+5 — exactly vanilla Nord's
            //     Two-Handed +10 and One-Handed / Block / Smithing / Speech /
            //     Light Armor +5 — plus height 1.03 and weight 1.0.
            //   * `ElderRace` decodes to seven `0xFF` Skill_None slots, which
            //     is correct: the Elder race grants no skill bonuses.
            // Both are strong evidence for the offsets *and* the skill-index
            // mapping, which a length check alone could not establish.
            //
            // Note the pair count differs from the TES4 arm above: TES5 has 7
            // skill slots plus two padding bytes where TES4/FO3/FNV has 8.
            b"DATA" if matches!(game, GameKind::Skyrim) && matches!(sub.data.len(), 128 | 164) => {
                let mut r = SubReader::new(&sub.data);
                for _ in 0..7 {
                    let skill = r.u8_or_default();
                    let bonus = r.u8_or_default() as i8;
                    // Same Skill_None handling as the TES4 arm — keep the
                    // public Vec "real bonuses only".
                    if skill != 0xFF {
                        record.skill_bonuses.push((skill, bonus));
                    }
                }
                let _padding = r.u16_or_default();
                let h_m = r.f32_or_default();
                let h_f = r.f32_or_default();
                let w_m = r.f32_or_default();
                let w_f = r.f32_or_default();
                record.base_height = (h_m, h_f);
                record.base_weight = (w_m, w_f);
                record.race_flags = r.u32_or_default();
                let starting_health = r.f32_or_default();
                if starting_health.is_finite() && starting_health > 0.0 {
                    record.starting_health = Some(starting_health);
                }
                let starting_magicka = r.f32_or_default();
                if starting_magicka.is_finite() && starting_magicka > 0.0 {
                    record.starting_magicka = Some(starting_magicka);
                }
                let starting_stamina = r.f32_or_default();
                if starting_stamina.is_finite() && starting_stamina > 0.0 {
                    record.starting_stamina = Some(starting_stamina);
                }
            }
            // DATA (FO4 200 B / FO76 216 B) — a third layout again, and
            // deliberately only partially decoded.
            //
            // Measured against vanilla `Fallout4.esm` (45 RACE records) and
            // `SeventySix.esm` (157) on 2026-08-12:
            //   * There is **no skill-bonus array at all** — the record opens
            //     with floats at offset 0. That is consistent with the games
            //     themselves: neither FO4 nor FO76 has skills. Feeding these
            //     bytes through the TES5 arm would read `00 00 80 3f` as the
            //     pairs `(0,0) (128,63)` — precisely the garbage #1629 removed.
            //   * `heightMale` / `heightFemale` sit at offsets 0 and 4, and
            //     they carry real signal: `HumanRace` is 1.0 / 0.98 while
            //     `HumanChildRace` and `GhoulChildRace` are 0.825 / 0.825.
            //     A child race being shorter is what identifies the field.
            //   * Offsets 8..32 are **byte-identical across every shipped race
            //     in both games** (`0.5, 0.5, 0.0` twice). Being invariant they
            //     carry no information to validate an interpretation against,
            //     so they are left undecoded rather than guessed. The shape
            //     suggests FO4's three-axis (thin / muscular / large) body
            //     morph per gender, which `base_weight`'s `(male, female)`
            //     pair could not represent even if it were confirmed — so
            //     `base_weight` stays at its default here, by design.
            // A `race_flags`-shaped word sits at offset 32 as it does in TES5,
            // but its bit meanings are unverified for these games (FO4's
            // non-playable `HumanChildRace` has bit 0 set, which contradicts
            // the TES-lineage "bit 0 = Playable"), so it is not surfaced.
            b"DATA"
                if matches!(game, GameKind::Fallout4 | GameKind::Fallout76)
                    && sub.data.len() >= 8 =>
            {
                let mut r = SubReader::new(&sub.data);
                record.base_height = (r.f32_or_default(), r.f32_or_default());
            }
            // Any DATA that reached here is a shape no arm claims. Log it
            // rather than dropping it silently: the whole point of #1629 /
            // #2455 is that a RACE record quietly left at defaults reads
            // identically to one that decoded fine. Mirrors the
            // `xcll_size_sanity_warn` pattern. `debug` not `warn` because
            // Starfield legitimately ships zero RACE DATA sub-records, so a
            // louder level would be pure noise on a correct load.
            b"DATA" => {
                log::debug!(
                    "RACE {:08X}: DATA sub-record ({} bytes) has no decoder for {:?} — \
                     skill bonuses / height / weight / flags stay at defaults",
                    form_id,
                    sub.data.len(),
                    game,
                );
            }
            // MODL appears multiple times in RACE for body parts. Collect them all.
            //
            // FNV / FO3: each MODL is preceded by an INDX naming the
            // body part. Pair them so the spawner can pick out the
            // eyes (INDX 7 / 8) without guessing by list position.
            // `gender_section` tracks the MNAM / FNAM split so the
            // spawner can pick gender-appropriate variants.
            b"INDX" if sub.data.len() >= 4 => {
                pending_indx = Some(SubReader::new(&sub.data).u32_or_default());
                icon_indx = pending_indx;
            }
            b"ICON" if in_head_section => {
                if let Some(idx) = icon_indx.take() {
                    record
                        .head_part_textures
                        .push((idx, read_zstring(&sub.data), gender_section));
                }
            }
            // #5487 — the body section's own ICON run (0 UpperBody…
            // 4 Tail, per-gender). `icon_indx` is armed by the same
            // INDX arm above; NAM1 resets it, so the first body ICON
            // only lands here after its own INDX.
            b"ICON" if !in_head_section && !in_skeleton_section => {
                if let Some(idx) = icon_indx.take() {
                    record
                        .body_part_textures
                        .push((idx, read_zstring(&sub.data), gender_section));
                }
            }
            b"MNAM" => {
                gender_section = Some(0); // Male
            }
            b"FNAM" => {
                gender_section = Some(1); // Female
            }
            b"ANAM" if game == GameKind::Skyrim && in_skeleton_section => {
                if let Some(gender) = gender_section {
                    record.skeleton_models[gender as usize] = read_zstring(&sub.data);
                }
            }
            b"NAM3" => {
                in_skeleton_section = false;
            }
            // Head-data marker: opens the INDX/MODL run whose indices
            // are head-part roles. Oblivion authors it with no gender
            // markers at all (one shared run of 0..8); FO3 / FNV split
            // it MNAM / FNAM. #3419.
            b"NAM0" => {
                in_skeleton_section = false;
                in_head_section = true;
                gender_section = None;
                pending_indx = None;
                icon_indx = None;
            }
            // Body-data marker: closes the head section. Its INDX run
            // restarts at 0 for a different vocabulary (upper body /
            // hands / `.egt`), so nothing past here belongs in
            // `head_parts`. #3419.
            b"NAM1" => {
                in_skeleton_section = false;
                in_head_section = false;
                gender_section = None;
                pending_indx = None;
                icon_indx = None;
            }
            b"MODL" => {
                let path = read_zstring(&sub.data);
                if let Some(idx) = pending_indx.take() {
                    if in_head_section {
                        record.head_parts.push((idx, path.clone(), gender_section));
                    }
                }
                record.body_models.push(path);
            }
            // ── Oblivion-only sub-records (#967 / OBL-D3-NEW-03) ───────
            // Plumbed under `is_oblivion` because TES5+ reuses these
            // FourCCs with different payloads (e.g. TES5 VNAM is a
            // 4-byte u32 instead of TES4's two form IDs at 8 bytes).
            // Gating on `GameKind::Oblivion` avoids cross-game
            // misreads when a future loader walks the same arm.
            b"ATTR" if is_oblivion && sub.data.len() >= 16 => {
                let mut attrs = RaceAttributes::default();
                attrs.male.strength = sub.data[0];
                attrs.male.intelligence = sub.data[1];
                attrs.male.willpower = sub.data[2];
                attrs.male.agility = sub.data[3];
                attrs.male.speed = sub.data[4];
                attrs.male.endurance = sub.data[5];
                attrs.male.personality = sub.data[6];
                attrs.male.luck = sub.data[7];
                attrs.female.strength = sub.data[8];
                attrs.female.intelligence = sub.data[9];
                attrs.female.willpower = sub.data[10];
                attrs.female.agility = sub.data[11];
                attrs.female.speed = sub.data[12];
                attrs.female.endurance = sub.data[13];
                attrs.female.personality = sub.data[14];
                attrs.female.luck = sub.data[15];
                record.base_attributes = Some(attrs);
            }
            b"DNAM" if is_oblivion && sub.data.len() >= 8 => {
                let mut r = SubReader::new(&sub.data);
                let male = r.u32_or_default();
                let female = r.u32_or_default();
                record.default_hair = Some((male, female));
            }
            b"VNAM" if is_oblivion && sub.data.len() >= 8 => {
                let mut r = SubReader::new(&sub.data);
                let male = r.u32_or_default();
                let female = r.u32_or_default();
                record.voice_forms = Some((male, female));
            }
            b"PNAM" if is_oblivion && sub.data.len() >= 4 => {
                record.facegen_main_clamp = Some(SubReader::new(&sub.data).f32_or_default());
            }
            b"UNAM" if is_oblivion && sub.data.len() >= 4 => {
                record.facegen_face_clamp = Some(SubReader::new(&sub.data).f32_or_default());
            }
            b"XNAM" if is_oblivion && sub.data.len() >= 8 => {
                let mut r = SubReader::new(&sub.data);
                let other_race = remap_fid(r.u32_or_default(), remap);
                let adjustment = r.i32_or_default();
                record.race_reactions.push((other_race, adjustment));
            }
            // CNAM intentionally skipped — its 4-byte payload mixes a
            // bitmask + 2-byte field that OpenMW also skips (see
            // `esm4/loadrace.cpp:232-251`). Authoritative semantics
            // are undocumented; revisit when M41.0 Phase 3b needs it.
            //
            // WNAM — default skin ARMO, Skyrim+ only (#2093 /
            // SKY-D3-NEW-01). Gated on `uses_prebaked_facegen()`
            // rather than a hardcoded game list so FO76/Starfield ride
            // along automatically; TES4/FO3/FNV RACE records don't
            // author WNAM at all.
            b"WNAM" if game.uses_prebaked_facegen() && sub.data.len() >= 4 => {
                let raw = SubReader::new(&sub.data).u32_or_default();
                record.default_skin = Some(remap_fid(raw, remap));
            }
            _ => {}
        }
    }

    record
}
