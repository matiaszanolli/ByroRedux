//! The placed-reference family (REFR / ACHR / ACRE payloads), extracted
//! from ../mod.rs (#5805 type-hub split).
//!
//! Types: `CellOwnership`, `PlacedRef`, `RagdollPoseBone`, `LockData`,
//! `TextureSlotSwap`, `TeleportDest`, `PrimitiveBounds`, `LinkedRef`,
//! `ActivateParent`, `PortalLink`, `EnableParent`.

use crate::esm::records::script_instance::ScriptInstanceData;

/// Ownership tuple from `XOWN` / `XRNK` / `XGLB` sub-records. Lives
/// on both CELL records (whole-cell ownership) and REFR records
/// (per-placed-object override). All three fields can appear
/// independently — XRNK and XGLB are gates on top of the base XOWN
/// owner, but Bethesda content also ships XOWN-only cells (no rank
/// requirement, no global gate). See #692 / O3-N-04.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellOwnership {
    /// XOWN — owner FormID. References either an NPC_ (individual
    /// owner like a homeowner) or a FACT (faction-owned, like the
    /// Imperial Legion barracks). 0 / `None` would be unowned.
    pub owner_form_id: u32,
    /// XRNK — minimum faction rank required to bypass the ownership
    /// check. Only meaningful when `owner_form_id` references a FACT;
    /// `None` means no rank gate (any faction member counts).
    pub faction_rank: Option<i32>,
    /// XGLB — global variable FormID that controls ownership state
    /// at runtime. When set, the gameplay layer evaluates the global
    /// before applying ownership (e.g. quest-gated houses that the
    /// player can claim only after a stage). `None` means the
    /// ownership is always-active.
    pub global_var_form_id: Option<u32>,
}

/// A placed object reference within a cell (REFR or ACHR).
#[derive(Debug, Clone)]
pub struct PlacedRef {
    /// The REFR's own form ID (the placement-level identity).
    /// Distinct from [`base_form_id`] (which is the STAT / FURN /
    /// LIGH base record this REFR points at). Used by the FO4+
    /// PreCombined Mesh filter (#1188) to skip REFRs absorbed into
    /// `_oc.nif` files; the baked-ref list in `CellData::absorbed_ref_bakes`
    /// is keyed on this field, NOT [`base_form_id`]. Defaults to 0
    /// on legacy fixtures that pre-date the field.
    pub form_id: u32,
    pub base_form_id: u32,
    /// The ESM group-type this placement's REFR/ACHR/ACRE record was found
    /// directly under: `8` = Cell Persistent Children, `9` = Cell
    /// Temporary Children, `10` = Cell Visible Distant Children. `0xFF` on
    /// legacy fixtures/tests that pre-date this field and don't care.
    ///
    /// `6` (`Cell Children`) is the *container* and is not a membership
    /// value — it should never reach a parsed placement. #4169: it did,
    /// for every placement in every shipped master, because the walker
    /// captured the outer container's type once and threaded it through
    /// its own recursion without re-reading the nested group headers. The
    /// legend on this field said `6 = temporary` and agreed with the bug,
    /// so both read as intentional. A parent→child GRUP census over the
    /// five shipped masters finds 8/9/10 exclusively *inside* a type 6 and
    /// never beside it, and zero records directly inside a type 6.
    ///
    /// Not consumed anywhere yet — the engine's only "persistent" concept
    /// today is the worldspace-level persistent CELL
    /// (`byroredux/src/streaming/`), not per-REFR group membership. This
    /// is the placement metadata a future streaming system (what stays
    /// resident across cell transitions) and a save system (what must be
    /// distinguished on restore) will need. Previously discarded entirely
    /// at parse time — `parse_cell_group_inner` / `parse_wrld_children_inner`
    /// matched `6 | 8 | 9` with one shared arm and passed the same `&mut
    /// refs` for all three, so no downstream consumer could ever recover
    /// which group a given REFR came from. See #3728.
    pub group_type: u8,
    /// Position in Bethesda units (Z-up).
    pub position: [f32; 3],
    /// Euler rotation in radians (X, Y, Z).
    pub rotation: [f32; 3],
    pub scale: f32,
    /// Enable-parent gating from the REFR's `XESP` sub-record. When
    /// `Some`, the REFR's spawn visibility is controlled by another
    /// REFR's enable state (and optionally inverted). `None` means the
    /// REFR is unconditionally placed at cell-load time.
    ///
    /// The cell loader uses this to skip REFRs that are default-
    /// disabled (XESP set + not inverted) — pre-#349 every "spawn after
    /// quest stage" REFR rendered immediately on cell load. See #349.
    pub enable_parent: Option<EnableParent>,
    /// Placement-scoped `REPU` override from the REFR's `WMI1` sub-record
    /// (#3325) — the same faction → reputation edge [`FactionRecord`] carries,
    /// but bound to this one placement rather than the base faction. 36 of
    /// `FalloutNV.esm`'s 82 `WMI1` payloads are REFR-scoped like this, and
    /// all 36 resolve to a real `REPU` record.
    ///
    /// [`FactionRecord`]: crate::esm::records::FactionRecord
    pub reputation_ref: Option<u32>,
    /// Teleport destination from the REFR's `XTEL` sub-record. When
    /// present, this REFR is a door whose activation transports the
    /// player to the destination REFR's position and rotation. Pre-#412
    /// every interior door was dead on activation. See audit FO4-D6-B1.
    pub teleport: Option<TeleportDest>,
    /// Primitive bounds / shape from the REFR's `XPRM` sub-record.
    /// Trigger volumes and invisible activators carry no MODL and are
    /// defined entirely by this primitive (box / sphere / plane / line).
    /// Pre-#412 triggers were invisible in the editor view and had no
    /// activation volume at runtime.
    pub primitive: Option<PrimitiveBounds>,
    /// Linked refs from the REFR's `XLKR` sub-records. Multiple allowed
    /// per REFR. Each pair is `(keyword_form_id, target_ref_form_id)`;
    /// the keyword (`null` if untyped) tags the relationship kind (e.g.
    /// NPC ↔ patrol marker, door ↔ teleport partner, switch ↔ light).
    /// Pre-#412 NPCs didn't patrol and doors didn't pair.
    pub linked_refs: Vec<LinkedRef>,
    /// The REFR's activate parents (`XAPR`, xEdit `wbREFR`): references that "pass
    /// on" an activation to this one (CK *Reference*, Activate Parents).
    pub activate_parents: Vec<ActivateParent>,
    /// `XAPD` Parent Activate Only: "this reference can ONLY be activated by an
    /// Activate Parent - direct activation (e.g. by the player) will not be
    /// possible. Scripts can always activate the object".
    pub parent_activate_only: bool,
    /// `XLRT` location-reference types attached to this placement. Skyrim
    /// quest aliases with `ALRT` select live references by these `LCRT`
    /// FormIDs (for example MQ101's distinct prisoner/soldier roles).
    pub location_ref_types: Vec<u32>,
    /// Room membership form IDs from the REFR's `XRMR` sub-record —
    /// the room(s) this ref belongs to for FO4 cell-subdivided interior
    /// culling. Empty when the REFR is not room-scoped. See #412.
    pub rooms: Vec<u32>,
    /// Portal connections from the REFR's `XPOD` sub-record. Each pair
    /// is `(origin_room_ref, destination_room_ref)`. Portal REFRs gate
    /// the interior room-to-room visibility graph. Pre-#412 this was
    /// silently dropped so any vault using portals couldn't cull.
    pub portals: Vec<PortalLink>,
    /// LIGH radius override from the REFR's `XRDS` sub-record. When
    /// `Some`, overrides the base LIGH record's radius for per-ref
    /// light tuning (bulb placement, lantern fine-tune).
    pub radius_override: Option<f32>,
    /// Alternate texture set from the REFR's `XATO` sub-record — a
    /// TXST FormID that overrides the base mesh's NIF-authored textures
    /// at this placement. Common on FO4 weapons / armor / signage that
    /// share one model but ship multiple TXST variants. The cell loader
    /// resolves this against `EsmCellIndex.texture_sets` and overlays
    /// the 8 TX00–TX07 slots (plus MNAM → BGSM chain) onto the spawned
    /// mesh. Pre-#584 37 % of vanilla FO4 TXSTs (140 / 382) were parsed
    /// but never consumed because the REFR parser dropped this field.
    /// See audit FO4-DIM6-02.
    pub alt_texture_ref: Option<u32>,
    /// Land-scoped TXST override from the REFR's `XTNM` sub-record —
    /// a TXST FormID that overrides an LTEX default on landscape
    /// references. Wire layout is identical to XATO (single u32 FormID)
    /// but targets LAND records, not models. Parsed for completeness;
    /// the LAND-override consumer path is separate from the mesh path
    /// wired for XATO in this issue. See audit FO4-DIM6-02.
    pub land_texture_ref: Option<u32>,
    /// Per-slot texture swaps from the REFR's `XTXR` sub-records. Each
    /// pair is `(txst_form_id, slot_index)` where `slot_index` picks
    /// one of TX00..TX07 — letting a REFR override a single slot of
    /// the base mesh's texture set without replacing the whole TXST.
    /// Multiple XTXR sub-records allowed per REFR; the cell loader
    /// applies them in authoring order (later wins for the same slot).
    pub texture_slot_swaps: Vec<TextureSlotSwap>,
    /// LIGH FormID attached via the REFR's `XEMI` sub-record — a
    /// per-placement emissive light that the renderer should spawn on
    /// top of (or instead of) the base LIGH. Parsed for completeness;
    /// the consumer wiring (per-REFR emissive light spawn) is follow-up
    /// work. See audit FO4-DIM6-02.
    pub emissive_light_ref: Option<u32>,
    /// MSWP FormID attached via the REFR's `XMSP` sub-record — a
    /// material-swap table the cell loader resolves against
    /// `EsmCellIndex.material_swaps` to produce per-REFR BGSM/BGEM
    /// substitutions on the base mesh's authored material slots.
    /// Pre-#971 every Raider armour colour-variant, settlement clutter
    /// variation, station-wagon rust pattern, and Vault decay overlay
    /// rendered with the base mesh's textures because this field was
    /// silently dropped at parse time. See audit FO4-D4-NEW-08.
    pub material_swap_ref: Option<u32>,
    /// Per-REFR ownership override (XOWN / XRNK / XGLB). When `Some`,
    /// the placed object (chest, bed, individual storage) is owned
    /// independently of the parent cell's ownership — stealing the
    /// item is a crime even in a public cell. See `CellOwnership`
    /// docs and #692.
    pub ownership: Option<CellOwnership>,
    /// Decoded `VMAD` on the placed reference ITSELF (Skyrim+ — the
    /// objectReference Papyrus-attached data). `None` when the REFR has no
    /// `VMAD`. Distinct from the base record's `VMAD`: this carries the
    /// *override* scripts a uniquely-scripted placement adds (a quest
    /// lever, a one-off activator). The cell loader attaches these
    /// additively, alongside the base record's scripts. Pre-#1737 the REFR
    /// walker only flagged presence and dropped the decoded scripts, so
    /// this whole class of placed-reference scripting attached nothing.
    /// See audit SCR-D7-01.
    pub script_instance: Option<ScriptInstanceData>,
    /// Lock state from the REFR's `XLOC` sub-record. `Some` iff the
    /// placement author checked "Locked" in the Creation Kit / GECK —
    /// the subrecord is entirely absent otherwise, so presence alone
    /// means locked (confirmed against `openmw`'s `ESM4::Reference`
    /// reader, which sets `mIsLocked = true` unconditionally on `XLOC`
    /// presence). Applies to both doors and containers; `None` means
    /// unlocked / not applicable. See #3098.
    pub lock: Option<LockData>,
    /// Authored local water-current velocity from entry 0 of the REFR `XWCU`
    /// array, in Gamebryo Z-up coordinates. Vanilla Skyrim authors it on
    /// placed water activators, mirroring their WATR `NAM0`; current-marker
    /// references use it alongside their placement bounds. `None` means the
    /// reference does not author a local current. CELL-level `XWCU` is not
    /// read: its shipped entries carry no velocity (see `xwcu_linear_velocity`).
    pub water_velocity: Option<[f32; 3]>,
    /// Placed stack size from the REFR's `XCNT` (#4706): `Some(n)` for an
    /// item placement authored as a stack of `n` (vanilla: 2–50, e.g. FO3's
    /// stacks of 12 rounds). `None` when absent — a single item.
    pub item_count: Option<u32>,
    /// Record-header "Initially Disabled" flag (0x800, #4813): the placement
    /// loads disabled until a script `Enable()`s it. Census of refs carrying
    /// it with no `XESP` (so the state is unambiguous): 149 non-actor refs on
    /// FNV, 440 on Skyrim, 168 on Oblivion, plus hundreds of hostile actors.
    /// The cell loader treats it as the reference's authored default, which
    /// a scripted `Enable()`/`Disable()` in `ReferenceEnableState` overrides.
    pub initially_disabled: bool,
    /// `ACHR` "Starts Dead" flag (0x200, #4814): an authored corpse. Set only
    /// for `ACHR` on the 24-byte-header family; Skyrim (1 123 refs) and FO4
    /// (1 178) use it for every placed corpse.
    ///
    /// This placement-level bit is NOT the corpse marker everywhere. Each
    /// remaining game marks corpses differently and is handled at its own
    /// layer: Oblivion stamps the BASE actor's `NPC_`/`CREA` header
    /// (`0x80000`, decoded onto `NpcRecord::starts_dead` — #5013), and
    /// FO3/FNV corpses carry no placement marker at all — their marker is
    /// the BASE's `DATA` health ≤ 0 (decoded onto
    /// `NpcRecord::starts_dead` — #5005; xNVSE `GameObjects.h:99`
    /// "IsDead = HasNoHealth (baseForm health <= 0 …)" plus the CS wiki
    /// "Creating Dead Actors" rule it descends from), while the placement's
    /// `XRGD` is the authored ragdoll POSE (decoded onto
    /// [`Self::ragdoll_pose`]), not a death signal — vanilla places live
    /// posed actors over `XRGD` (Fisto, `VHDKimballRangerAmbushed01`).
    /// Zero here is therefore "no placement-level marker", not "not a
    /// corpse".
    pub starts_dead: bool,
    /// `ACHR` "Starts Unconscious" flag (0x2000, #5017), FO4 / FO76 /
    /// Starfield only (see `FLAG_STARTS_UNCONSCIOUS`). Vanilla sets it on
    /// dormant robots and turrets that a terminal, pod or quest script
    /// wakes. The actor job spawns such an actor unconscious
    /// (`ActorControlState::unconscious`, `apply_starts_unconscious`), with
    /// the runtime meaning from the Creation Kit's `SetUnconscious - Actor`
    /// page. That page is Skyrim's; FO4 documents only the one-liner, and the
    /// user approved the cross-game reading.
    pub starts_unconscious: bool,
    /// Authored Havok pose from the placement's `XRGD` (#5015), in file
    /// order; empty when the sub-record is absent. xEdit defines it on
    /// ACHR/ACRE and on REFR, and vanilla uses both. On actors it is the
    /// corpse pose: every posed ACHR is a Starts Dead ACHR, 1,097 in
    /// `Skyrim.esm` and 1,090 in `Fallout4.esm`; FO3/FNV corpses are covered
    /// by #5005. On REFRs it is the settled pose of multi-body clutter such
    /// as books, misc items and moveable statics: 7,420 REFRs in Skyrim and
    /// 7,829 in FO4. The engine poses a humanoid corpse's skeleton from it
    /// before its ragdoll activates (`byroredux/src/ragdoll_pose.rs`, #5015;
    /// the layout is derived, not sourced — see [`RagdollPoseBone`]); other
    /// skeletons and the FO4 corpses are not posed yet.
    pub ragdoll_pose: Vec<RagdollPoseBone>,
    /// M47.2 — a map marker's `FNAM` flags (`0x01` Visible, `0x02` Can
    /// Travel To, `0x04` "Show All" Hidden — xEdit's REFR *Map Marker*
    /// block, `XMRK` then `FNAM`); `None` when the reference is no map
    /// marker.
    pub map_marker: Option<u8>,
    /// `XEZN` — the reference's own `ECZN` (CK: a reference can be tagged
    /// with an encounter zone of its own); `None` falls back to its cell's
    /// [`CellData::encounter_zone_form`](crate::esm::cell::CellData::encounter_zone_form). `CalculateEncounterLevel` reads it.
    pub encounter_zone_form: Option<u32>,
}

impl PlacedRef {
    /// A bare placement of `base_form_id` at a Z-up pose: unit scale and no
    /// flags, links, ownership or scripts — what a runtime-created reference
    /// (M47.2's Created Object aliases) and test fixtures start from.
    pub fn at(form_id: u32, base_form_id: u32, position: [f32; 3], rotation: [f32; 3]) -> Self {
        Self {
            form_id,
            base_form_id,
            group_type: 0xFF,
            position,
            rotation,
            scale: 1.0,
            enable_parent: None,
            teleport: None,
            reputation_ref: None,
            primitive: None,
            linked_refs: Vec::new(),
            activate_parents: Vec::new(),
            parent_activate_only: false,
            location_ref_types: Vec::new(),
            rooms: Vec::new(),
            portals: Vec::new(),
            radius_override: None,
            alt_texture_ref: None,
            land_texture_ref: None,
            texture_slot_swaps: Vec::new(),
            emissive_light_ref: None,
            material_swap_ref: None,
            ownership: None,
            script_instance: None,
            lock: None,
            water_velocity: None,
            item_count: None,
            initially_disabled: false,
            starts_dead: false,
            starts_unconscious: false,
            ragdoll_pose: Vec::new(),
            map_marker: None,
            encounter_zone_form: None,
        }
    }
}

/// One 28-byte entry of a placement's `XRGD` ragdoll pose (#5015).
///
/// Wire layout from xEdit's shared `wbRagdoll` (`wbDefinitionsCommon.pas`),
/// the same on every game that carries it: `Bone Id` (u8) + 3 unused bytes +
/// `wbVec3PosRot` (position `f32×3`, then rotation `f32×3` in radians, like
/// `DATA`).
///
/// Only the layout is sourced. A byte census of the vanilla masters
/// (2026-09-29) shows that the fields are **not** what xEdit's labels
/// suggest, and a consumer must not assume they are:
/// - `bone_id` is not a unique key. A clutter REFR's entries often all carry
///   id 0; a Skyrim book has two, one per half. In an actor pose, entry 0 is a
///   root placement at position
///   `(0, 0, 0)`. The remaining entries of a standard humanoid pose carry
///   ids 0..=16 once each, and entry 0 repeats one of them.
/// - Positions are parent-relative bone offsets (thigh→calf ≈ 31, calf→foot
///   ≈ 28), not world or ref-relative coordinates. The bone axis is +X on
///   FO3/FNV and +Z on Skyrim.
/// - The Euler order and the entry→skeleton-body mapping are undocumented, but
///   were recovered from the Skyrim corpus for the humanoid ragdoll
///   (`byroredux/src/ragdoll_pose.rs`, #5015): entry `k` is the skeleton NIF's
///   ragdoll body `k` (`bone_id` is not that order), the rotation is the
///   Gamebryo XYZ product with negated angles, and entry 0 is the centre of
///   mass at the placement. A trailing `(0, 17, 10)` entry with zero rotation
///   is not a body. Those are interpretations, not a source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RagdollPoseBone {
    pub bone_id: u8,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
}

/// Lock state decoded from a REFR's `XLOC` sub-record.
///
/// Wire layout (12-byte common prefix, cross-game): `lock_level(u8)` +
/// 3 unused bytes + `key(FormID, u32, 0 = none)` + `flags(u8)` + 3
/// unused bytes. Oblivion sometimes pads to 16 bytes and Skyrim/FO3
/// pad to 20 with additional trailing bytes of unconfirmed purpose
/// (cross-checked against `openmw`'s `ESM4::Reference::load`, whose own
/// comment flags the trailing byte as `// flag?` — not just this
/// engine's uncertainty). We read the confirmed 12-byte prefix and
/// ignore anything past it, the same tolerant-prefix approach already
/// used for `XTEL` above.
///
/// Scope note (#3098): this is the **parse-and-store** half only. Key
/// checks and the lockpicking minigame are deferred to a future
/// milestone; today the only consumer policy is "locked ⇒ not
/// activatable" (see `byroredux/src/interaction.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockData {
    /// Lock difficulty (0 = trivial .. 100 = master-tier, roughly;
    /// exact per-game tier boundaries are a gameplay-policy concern,
    /// not a parse-time one).
    pub lock_level: u8,
    /// FormID of the key that opens this lock, if any. `None` when the
    /// raw FormID is 0 (no key required — pure lockpick/level gate).
    pub key_form_id: Option<u32>,
    /// Raw flag byte trailing the key FormID. Bit semantics are not
    /// confirmed against a primary source (see struct doc) so this
    /// rides through unparsed rather than being guessed at.
    pub flags: u8,
}

/// Per-slot texture swap from one `XTXR` sub-record — a TXST form ID
/// paired with the index of the slot (0..=7) it replaces on the host
/// mesh's texture set. Multiple XTXR sub-records allowed per REFR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureSlotSwap {
    /// TXST FormID supplying the replacement texture.
    pub texture_set: u32,
    /// Slot index to replace — 0 = TX00 (diffuse), 1 = TX01 (normal),
    /// … 7 = TX07 (specular). Values outside 0..=7 are clamped away at
    /// consumer side so a malformed XTXR can't crash the loader.
    pub slot_index: u32,
}

/// Teleport destination payload for `XTEL` — a door ref's target.
///
/// Wire layout (32 bytes): destination_ref(u32) + position(3×f32) +
/// rotation(3×f32) + flags(u32). Flags trail on Skyrim+ and may be
/// absent on older games; parsed conservatively to the first 28 bytes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TeleportDest {
    /// FormID of the destination REFR (another door, usually) in the
    /// target cell. The cell loader resolves this to coordinates and
    /// the player's teleport target.
    pub destination: u32,
    /// Destination position in Bethesda units (Z-up).
    pub position: [f32; 3],
    /// Destination Euler rotation in radians (X, Y, Z).
    pub rotation: [f32; 3],
}

/// Primitive shape payload for `XPRM` — invisible trigger / activator
/// geometry that doesn't ship a MODL. Conservative layout: first three
/// f32s are box bounds, next three are the editor visualization color,
/// then a trailing f32 + u32 (shape type enum). All in 32 bytes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrimitiveBounds {
    /// Box / shape extents in Bethesda units.
    pub bounds: [f32; 3],
    /// Editor visualization color (R, G, B) in 0..=1 range. Mostly
    /// useful for debug rendering; preserved so a future editor view
    /// can display the exact tint the original author chose.
    pub color: [f32; 3],
    /// Trailing f32 per UESP — semantic unclear (possibly an alpha or
    /// a second-axis parameter). Preserved verbatim for future use.
    pub unknown: f32,
    /// Shape type enum per UESP: 1=Box, 2=Line, 3=Sphere, 4=Portal,
    /// 5=Plane. Other values indicate a game-specific extension.
    pub shape_type: u32,
}

/// Linked-ref pair from a single `XLKR` sub-record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinkedRef {
    /// Keyword form ID categorizing the link (e.g. `LinkCarryable`,
    /// `LinkPatrol`). `0` / `0xFFFFFFFF` means untyped.
    pub keyword: u32,
    /// Target REFR form ID the keyword links this REFR to.
    pub target: u32,
}

/// One `XAPR` activate parent: the reference whose activation this one receives,
/// and the seconds to wait after it ("Delay").
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActivateParent {
    pub reference: u32,
    pub delay: f32,
}

/// Portal pair from a single `XPOD` sub-record. Portal REFRs connect
/// two rooms for FO4 / Skyrim-SE interior cell-subdivision culling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PortalLink {
    pub origin: u32,
    pub destination: u32,
}

/// REFR enable-parent gating from the `XESP` sub-record (present since
/// Oblivion — NOT Skyrim+; the `b"XESP"` match arm is unguarded by game
/// and Oblivion ACRE/ACHR carry it, per #349/#396/#2088).
/// Layout: 4-byte parent FormID + 1-byte flags. Bit 0 of the flags
/// inverts the enable state (so `inverted = true` means the REFR is
/// visible when the parent is *disabled*).
#[derive(Debug, Clone, Copy)]
pub struct EnableParent {
    /// FormID of the parent REFR whose enable state controls this
    /// REFR's visibility.
    pub form_id: u32,
    /// When `true`, this REFR is visible when the parent is disabled
    /// (and hidden when the parent is enabled).
    pub inverted: bool,
}

impl EnableParent {
    /// Returns `true` if the REFR should be skipped at cell-load time
    /// because its parent's assumed initial state would hide it.
    ///
    /// Interim predicate (#471): without a two-pass loader that can
    /// consult the parent REFR's own "initial disabled" flag
    /// (bit 0x0800), we assume the common case — parents are
    /// persistent, always-enabled actors / statics. Under that
    /// assumption:
    ///   - non-inverted XESP: child visible iff parent enabled → render
    ///   - inverted XESP:     child visible iff parent disabled → skip
    ///
    /// #349's original predicate had the sense flipped and hid every
    /// non-inverted XESP child, wiping out most quest-enabled clutter,
    /// shop inventory, and patrol markers on FNV / FO3 cells. False
    /// negatives on the rarer "supposed to be hidden" XESP chains are
    /// visually less bad than wholesale invisibility.
    ///
    /// The long-term fix is a two-pass loader that builds a REFR flag
    /// table first, then applies XESP gating against each parent's real
    /// 0x0800 bit.
    pub fn default_disabled(self) -> bool {
        self.form_id != 0 && self.inverted
    }
}
