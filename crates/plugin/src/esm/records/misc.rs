//! Stub parsers for nine record types that were previously falling
//! through the `parse_esm` catch-all and getting skipped wholesale
//! (#458 / audit FO3-3-07). Each parser extracts enough data for
//! *references* into the record to resolve — typically EDID + a
//! handful of form refs + a couple of scalar fields — without doing
//! deep sub-record decoding. Full parsing of each can be tightened
//! up per-type when the consuming system lands.
//!
//! | Record | Used by | Minimal shape captured |
//! |--------|---------|------------------------|
//! | `WATR` | `CELL.XCWT` water type | EDID, FULL, TNAM texture |
//! | `NAVI` | nav mesh master | EDID, NVER version |
//! | `NAVM` | per-cell nav mesh | EDID, NVER version |
//! | `REGN` | region definition | EDID, WNAM weather, RCLR color |
//! | `ECZN` | encounter zone | EDID, owner / rank / flags / min-level |
//! | `LGTM` | lighting template | EDID + XCLL-shaped DATA block |
//! | `HDPT` | head part (FaceGen) | EDID, FULL, MODL |
//! | `EYES` | eye definition | EDID, FULL, ICON, flags |
//! | `HAIR` | hair definition | EDID, FULL, MODL, ICON, flags |
//!
//! Per-game bit layouts vary on the LGTM + DATA / HDPT / EYES / HAIR
//! records past Skyrim; the stubs parse the FO3/FNV byte layout and
//! gracefully return defaults on short buffers — Skyrim+ re-parsing
//! lands alongside the consuming system.

use super::common::{read_f32_at, read_lstring_or_zstring, read_u32_at, read_zstring};
use crate::esm::reader::SubRecord;

/// Water record — referenced by `CELL.XCWT` (water type form ID on a
/// cell). Pre-fix every XCWT reference dangled at cell load.
#[derive(Debug, Clone, Default)]
pub struct WatrRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// Diffuse / noise texture path (`TNAM`). Most FNV water types ship
    /// a `textures\water\*.dds` here.
    pub texture_path: String,
}

pub fn parse_watr(form_id: u32, subs: &[SubRecord]) -> WatrRecord {
    let mut out = WatrRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"TNAM" => out.texture_path = read_zstring(&sub.data),
            _ => {}
        }
    }
    out
}

/// Navigation mesh master record (`NAVI`). Skyrim+ splits navigation
/// metadata into a top-level master + per-cell `NAVM` children; for
/// pre-Skyrim games this is rare but still present on wilderness
/// worldspaces. Post-render, not a draw path.
#[derive(Debug, Clone, Default)]
pub struct NaviRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `NVER` version tag — format revision the mesh data was exported at.
    pub version: u32,
}

pub fn parse_navi(form_id: u32, subs: &[SubRecord]) -> NaviRecord {
    let mut out = NaviRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"NVER" if sub.data.len() >= 4 => {
                out.version = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            _ => {}
        }
    }
    out
}

/// Per-cell navigation mesh (`NAVM`). Geometry is not extracted — the
/// AI / pathfinding system lands separately and will need to re-parse
/// the full vertex + triangle + edge table.
#[derive(Debug, Clone, Default)]
pub struct NavmRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub version: u32,
}

pub fn parse_navm(form_id: u32, subs: &[SubRecord]) -> NavmRecord {
    let mut out = NavmRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"NVER" if sub.data.len() >= 4 => {
                out.version = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            _ => {}
        }
    }
    out
}

/// Region record (`REGN`). Tags a world-space area with a weather type,
/// a color tint, and downstream `RDAT` entries that scale spawn density
/// / map-color / ambient SFX. Only EDID + weather + color are captured
/// here; the `RDAT`-driven sub-records are out of scope.
#[derive(Debug, Clone, Default)]
pub struct RegnRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `WNAM` — weather form that this region enforces. `None` when the
    /// region inherits from its worldspace.
    pub weather_form: Option<u32>,
    /// `RCLR` — RGB region tint for map shading. Stored as raw u8[3];
    /// alpha byte (if any) is ignored.
    pub color: Option<[u8; 3]>,
}

pub fn parse_regn(form_id: u32, subs: &[SubRecord]) -> RegnRecord {
    let mut out = RegnRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"WNAM" if sub.data.len() >= 4 => {
                out.weather_form = read_u32_at(&sub.data, 0);
            }
            b"RCLR" if sub.data.len() >= 3 => {
                out.color = Some([sub.data[0], sub.data[1], sub.data[2]]);
            }
            _ => {}
        }
    }
    out
}

/// Encounter zone (`ECZN`). Governs spawn scaling / faction ownership
/// on the cells that reference it via `XEZN`. The `DATA` layout is:
/// `owner (u32 FormID) + rank (u8) + min-level (u8) + flags (u8) + unused (u8)`.
#[derive(Debug, Clone, Default)]
pub struct EcznRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Form ID of the faction or actor that owns this zone. `0` when
    /// the field is unset (wilderness zones sometimes leave it blank).
    pub owner_form: u32,
    /// Faction rank required; 0 = no rank gate.
    pub rank: u8,
    /// Minimum player level for zone to unlock spawn overrides.
    pub min_level: u8,
    pub flags: u8,
}

pub fn parse_eczn(form_id: u32, subs: &[SubRecord]) -> EcznRecord {
    let mut out = EcznRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"DATA" if sub.data.len() >= 7 => {
                out.owner_form = read_u32_at(&sub.data, 0).unwrap_or(0);
                out.rank = sub.data[4];
                out.min_level = sub.data[5];
                out.flags = sub.data[6];
            }
            _ => {}
        }
    }
    out
}

/// Lighting template (`LGTM`). Provides a named bundle of XCLL-shaped
/// lighting values that cells can reference via `XLGT` and selectively
/// override per-field. Full per-field inheritance fallback lands
/// alongside #379; this stub captures the XCLL-prefix bytes so the
/// consuming lookup has something to read.
///
/// The `DATA` sub-record mirrors XCLL byte-for-byte (bytes 0-39):
///   0-3:   ambient  (RGBA, byte order per cell.rs XCLL parser)
///   4-7:   directional (RGBA)
///   8-11:  fog color (RGBA)
///   12-15: fog near (f32)
///   16-19: fog far (f32)
///   20-23: rotation X (i32 degrees)
///   24-27: rotation Y (i32 degrees)
///   28-31: directional fade (f32)
///   32-35: fog clip (f32)
///   36-39: fog power (f32)
#[derive(Debug, Clone, Default)]
pub struct LgtmRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Ambient color normalised to [0, 1] RGB. XCLL uses RGB byte order
    /// (see `cell.rs` comment; corrected post-#389 revert).
    pub ambient: [f32; 3],
    pub directional: [f32; 3],
    pub fog_color: [f32; 3],
    pub fog_near: f32,
    pub fog_far: f32,
    pub directional_fade: Option<f32>,
    pub fog_clip: Option<f32>,
    pub fog_power: Option<f32>,
}

pub fn parse_lgtm(form_id: u32, subs: &[SubRecord]) -> LgtmRecord {
    let mut out = LgtmRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"DATA" if sub.data.len() >= 20 => {
                let d = &sub.data;
                out.ambient = [
                    d[0] as f32 / 255.0,
                    d[1] as f32 / 255.0,
                    d[2] as f32 / 255.0,
                ];
                out.directional = [
                    d[4] as f32 / 255.0,
                    d[5] as f32 / 255.0,
                    d[6] as f32 / 255.0,
                ];
                out.fog_color = [
                    d[8] as f32 / 255.0,
                    d[9] as f32 / 255.0,
                    d[10] as f32 / 255.0,
                ];
                out.fog_near = read_f32_at(d, 12).unwrap_or(0.0);
                out.fog_far = read_f32_at(d, 16).unwrap_or(0.0);
                if d.len() >= 32 {
                    out.directional_fade = read_f32_at(d, 28);
                }
                if d.len() >= 36 {
                    out.fog_clip = read_f32_at(d, 32);
                }
                if d.len() >= 40 {
                    out.fog_power = read_f32_at(d, 36);
                }
            }
            _ => {}
        }
    }
    out
}

/// Head part record (`HDPT`). Used by FaceGen to assemble NPC faces —
/// each part (head, mouth, ears, etc.) references a mesh + texture
/// set and a type flag. Pre-Skyrim head variation is sparse enough
/// that even a stub unblocks NPC_ head-part resolution.
#[derive(Debug, Clone, Default)]
pub struct HdptRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub model_path: String,
    /// Flags byte — bit layout differs per-game; stored raw. Skyrim
    /// repurposes this as a u8 with bits 0-2 encoding the head-part
    /// type slot (face/eyes/hair/beard/brow/scar).
    pub flags: u8,
}

pub fn parse_hdpt(form_id: u32, subs: &[SubRecord]) -> HdptRecord {
    let mut out = HdptRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"MODL" => out.model_path = read_zstring(&sub.data),
            b"DATA" if !sub.data.is_empty() => {
                out.flags = sub.data[0];
            }
            _ => {}
        }
    }
    out
}

/// Eye definition (`EYES`). Points NPC_ records at the eye texture +
/// displayable name. Post-Skyrim these are picked via `ALIA`/`HCLF`;
/// FO3 and FNV use this record directly.
#[derive(Debug, Clone, Default)]
pub struct EyesRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub icon_path: String,
    pub flags: u8,
}

pub fn parse_eyes(form_id: u32, subs: &[SubRecord]) -> EyesRecord {
    let mut out = EyesRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"ICON" => out.icon_path = read_zstring(&sub.data),
            b"DATA" if !sub.data.is_empty() => {
                out.flags = sub.data[0];
            }
            _ => {}
        }
    }
    out
}

/// Hair definition (`HAIR`). Each NPC_ references a hair form that
/// supplies the mesh + texture for the head's hair part.
#[derive(Debug, Clone, Default)]
pub struct HairRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub model_path: String,
    pub icon_path: String,
    pub flags: u8,
}

pub fn parse_hair(form_id: u32, subs: &[SubRecord]) -> HairRecord {
    let mut out = HairRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"MODL" => out.model_path = read_zstring(&sub.data),
            b"ICON" => out.icon_path = read_zstring(&sub.data),
            b"DATA" if !sub.data.is_empty() => {
                out.flags = sub.data[0];
            }
            _ => {}
        }
    }
    out
}

// ── AI / dialogue / effect stubs (#446, #447) ────────────────────────
//
// Extended set of record types that pre-#446/#447 fell through to the
// catch-all skip. Same philosophy as the WATR/NAVI/... stubs above:
// capture EDID + FULL + a handful of flags / form refs so that dangling
// cross-references resolve; full per-record decoding lands with the
// consuming subsystem (AI runtime, dialogue system, perk pipeline).

/// `PACK` AI package record. 30-procedure scheduling system
/// (guard patrols, merchant behavior, dialogue triggers, ambient
/// idles). `NpcRecord.ai_packages` carries PKID form refs; pre-#446
/// those dangled.
///
/// Only the PKDT header (package flags + procedure type) is captured
/// here — PSDT / PLDT / PKTG / PKCU / PKPA decoding lands with the
/// AI runtime per the `ai_packages_procedures.md` memo.
#[derive(Debug, Clone, Default)]
pub struct PackRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Flags bitfield from PKDT (schedule / location repeat / weapon
    /// draw / etc.). Low 16 bits on FO3/FNV, u32 on Skyrim+.
    pub package_flags: u32,
    /// Procedure type — index into the 30-procedure catalog
    /// (`Travel`, `Wander`, `Sandbox`, `Find`, `Escort`, `Follow`,
    /// `Patrol`, `Use Item At`, ...). Read from PKDT offset 4.
    pub procedure_type: u32,
}

pub fn parse_pack(form_id: u32, subs: &[SubRecord]) -> PackRecord {
    let mut out = PackRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"PKDT" if sub.data.len() >= 8 => {
                out.package_flags = read_u32_at(&sub.data, 0).unwrap_or(0);
                out.procedure_type = read_u32_at(&sub.data, 4).unwrap_or(0);
            }
            _ => {}
        }
    }
    out
}

/// `QUST` quest record. Lifecycle container for the Story Manager and
/// Radiant Story systems. Stages (QSDT), objectives (QOBJ), aliases
/// (ALST), conditions (CTDA), scripts (SCRI) are deferred; this stub
/// surfaces the quest's identity + a handful of scalar fields so the
/// `quest_alias_system.md` / `quest_story_manager.md` memos can start
/// tracking real counts.
#[derive(Debug, Clone, Default)]
pub struct QustRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// Optional FO3/FNV quest script reference (pre-Papyrus bytecode).
    pub script_ref: u32,
    /// Quest flags from DATA byte 0 (`Start Game Enabled`, `Allow
    /// Repeated Stages`, `Event Based`, ...).
    pub quest_flags: u8,
    /// Priority from DATA byte 1. Higher = displayed first in pip-boy.
    pub priority: u8,
}

pub fn parse_qust(form_id: u32, subs: &[SubRecord]) -> QustRecord {
    let mut out = QustRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"SCRI" if sub.data.len() >= 4 => {
                out.script_ref = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            b"DATA" if sub.data.len() >= 2 => {
                out.quest_flags = sub.data[0];
                out.priority = sub.data[1];
            }
            _ => {}
        }
    }
    out
}

/// `DIAL` dialogue topic record. Parent of INFO dialogue lines (which
/// live in a nested GRUP tree — tracked as a follow-up; the current
/// `extract_records` walker takes a single record type and can't
/// simultaneously emit DIAL + INFO). This stub captures the topic's
/// quest owners (QSTI refs, 4 bytes each) so NPC / quest systems can
/// enumerate topics without re-parsing.
#[derive(Debug, Clone, Default)]
pub struct DialRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// Quest form IDs that own this dialogue topic (one per QSTI
    /// sub-record). FO3/FNV topics often list multiple owners.
    pub quest_refs: Vec<u32>,
    /// INFO topic responses parsed from the DIAL's `Topic Children`
    /// sub-GRUP (group_type == 7). Pre-#631 the children were silently
    /// skipped because `extract_records` filters on a single record
    /// type; this field is now populated by the dedicated
    /// `extract_dial_with_info` walker. Each entry is one branch of the
    /// dialogue (a single NPC response + its conditions / triggers).
    pub infos: Vec<InfoRecord>,
}

/// `INFO` dialogue topic response. One per branch of an `NPC says X
/// when Y` choice tree, owned by the parent `DIAL` topic via the
/// nested Topic Children GRUP. Stub captures the response text +
/// type byte + sibling links so quest / dialogue systems can
/// enumerate branches without re-parsing. Conditions (CTDA),
/// scripts (SCHR/SCDA), and edits (NAM3) are deferred until the
/// condition runtime lands. See #631.
#[derive(Debug, Clone, Default)]
pub struct InfoRecord {
    pub form_id: u32,
    /// Response text shown / spoken to the player (NAM1).
    pub response_text: String,
    /// Designer notes — usually direction for the voice actor (NAM2).
    pub designer_notes: String,
    /// `TRDT` response-data byte 0 — `Response_Type` enum (Custom /
    /// Force Greet / etc. on FO3/FNV; Combat / Death / Hello etc. on
    /// Skyrim). Captured raw; mapping to the per-game enum is
    /// downstream consumer work. 0 when TRDT is absent.
    pub response_type: u8,
    /// `TCLT` topic-link ref — IDs of other DIAL topics that this
    /// branch routes the conversation to. Multiple TCLTs are
    /// concatenated.
    pub topic_links: Vec<u32>,
    /// `PNAM` previous-info ref — the prior INFO in this branch. 0
    /// means "this is the first response in the chain".
    pub previous_info: u32,
}

pub fn parse_dial(form_id: u32, subs: &[SubRecord]) -> DialRecord {
    let mut out = DialRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"QSTI" if sub.data.len() >= 4 => {
                if let Some(q) = read_u32_at(&sub.data, 0) {
                    out.quest_refs.push(q);
                }
            }
            _ => {}
        }
    }
    out
}

pub fn parse_info(form_id: u32, subs: &[SubRecord]) -> InfoRecord {
    let mut out = InfoRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"NAM1" => out.response_text = read_lstring_or_zstring(&sub.data),
            b"NAM2" => out.designer_notes = read_zstring(&sub.data),
            b"TRDT" if !sub.data.is_empty() => {
                out.response_type = sub.data[0];
            }
            b"TCLT" if sub.data.len() >= 4 => {
                if let Some(t) = read_u32_at(&sub.data, 0) {
                    out.topic_links.push(t);
                }
            }
            b"PNAM" if sub.data.len() >= 4 => {
                out.previous_info = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            _ => {}
        }
    }
    out
}

/// `MESG` message / popup record. Quest-tutorial banners and
/// interaction prompts. `DESC` carries the text; `QNAM` optionally
/// ties the message to a quest for clean-up on quest completion.
#[derive(Debug, Clone, Default)]
pub struct MesgRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// Owning quest form ID (optional) — message clears when quest
    /// completes.
    pub owner_quest: u32,
}

pub fn parse_mesg(form_id: u32, subs: &[SubRecord]) -> MesgRecord {
    let mut out = MesgRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"DESC" => out.description = read_lstring_or_zstring(&sub.data),
            b"QNAM" if sub.data.len() >= 4 => {
                out.owner_quest = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            _ => {}
        }
    }
    out
}

/// `PERK` perk / trait record. Holds the condition list + entry-point
/// tree that drives the `perk_system.md` / `perk_entry_points.md`
/// memos' ~120 catalog. Entry-point decoding (PRKE) is deferred —
/// lands with the condition pipeline. Stub captures identity + flags
/// so the perk catalog can be enumerated at load time.
#[derive(Debug, Clone, Default)]
pub struct PerkRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// Flags byte from DATA (playable / hidden / leveled / trait).
    pub perk_flags: u8,
}

pub fn parse_perk(form_id: u32, subs: &[SubRecord]) -> PerkRecord {
    let mut out = PerkRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"DESC" => out.description = read_lstring_or_zstring(&sub.data),
            b"DATA" if !sub.data.is_empty() => {
                out.perk_flags = sub.data[0];
            }
            _ => {}
        }
    }
    out
}

/// `SPEL` spell / ability / power record. FO3/FNV also covers passive
/// abilities and radiation-poisoning style auto-cast effects. SPIT
/// carries cost + level requirement + flags; effect list (EFID/EFIT)
/// is deferred — lands with MGEF application.
#[derive(Debug, Clone, Default)]
pub struct SpelRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// Flags from SPIT offset 12 (or 8 on some pre-FNV variants).
    /// Bit 0 = `Manual Cost`, bit 2 = `Touch Explodes`.
    pub spell_flags: u32,
    /// Magicka cost from SPIT offset 0.
    pub cost: u32,
}

pub fn parse_spel(form_id: u32, subs: &[SubRecord]) -> SpelRecord {
    let mut out = SpelRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"SPIT" if sub.data.len() >= 16 => {
                out.cost = read_u32_at(&sub.data, 0).unwrap_or(0);
                out.spell_flags = read_u32_at(&sub.data, 12).unwrap_or(0);
            }
            _ => {}
        }
    }
    out
}

/// `MGEF` magic effect record. Universal bridge for Actor Value
/// modifications — every perk entry point, spell effect, and
/// ingredient effect routes through here. Full effect decoding is
/// deferred; the stub captures identity + flags so references from
/// SPEL / ALCH / INGR resolve at load time.
#[derive(Debug, Clone, Default)]
pub struct MgefRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// Flags from DATA offset 0 (hostile / recover / detrimental / ...).
    pub effect_flags: u32,
}

pub fn parse_mgef(form_id: u32, subs: &[SubRecord]) -> MgefRecord {
    let mut out = MgefRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"DESC" => out.description = read_lstring_or_zstring(&sub.data),
            b"DATA" if sub.data.len() >= 4 => {
                out.effect_flags = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            _ => {}
        }
    }
    out
}

/// `ACTI` activator record. FO3/FNV/Oblivion wall switches, buttons,
/// vending machines, lever-activated doors — anything that the player
/// "use"s but isn't a container, door, or NPC. SCRI on these records
/// runs the trigger script; DEST controls destruction-state meshes.
/// Full destruction-stage decoding is deferred — the stub captures
/// identity + model + SCRI cross-ref so dangling references resolve
/// at lookup time. See #521.
#[derive(Debug, Clone, Default)]
pub struct ActiRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// NIF path from MODL — already populated in `cells.statics` via
    /// the MODL catch-all, but duplicated here so a structured record
    /// map is internally consistent.
    pub model_path: String,
    /// SCRI — script form ID attached to this activator. `0` = no
    /// script. Referenced by trigger-system dispatch once it lands.
    pub script_form_id: u32,
    /// SNAM — sound form ID played on activation (optional).
    pub sound_form_id: u32,
    /// RADR / RNAM — radio station form ID, applicable to FNV radio
    /// transmitters (activator variant). `0` when absent.
    pub radio_form_id: u32,
}

pub fn parse_acti(form_id: u32, subs: &[SubRecord]) -> ActiRecord {
    let mut out = ActiRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"MODL" => out.model_path = read_zstring(&sub.data),
            b"SCRI" => out.script_form_id = read_u32_at(&sub.data, 0).unwrap_or(0),
            b"SNAM" => out.sound_form_id = read_u32_at(&sub.data, 0).unwrap_or(0),
            b"RNAM" | b"RADR" => {
                out.radio_form_id = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            _ => {}
        }
    }
    out
}

/// `TERM` terminal record — FO3/FNV computer consoles. Carries a
/// menu tree (MNAM entries), password (ANAM), body text (DNAM), and
/// the NIF model of the physical terminal. MNAM text is collected
/// into `menu_items` so a future terminal-interaction system can
/// walk the options without re-parsing. See #521.
#[derive(Debug, Clone, Default)]
pub struct TermRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub model_path: String,
    /// SCRI — script form ID (some terminals trigger quest advance
    /// scripts on successful hack).
    pub script_form_id: u32,
    /// ANAM — password string (may be empty for unlocked terminals).
    pub password: String,
    /// DNAM — footer / body text displayed on the terminal screen.
    pub footer_text: String,
    /// BSIZ — body-size scalar (u8, 0 = small, 1 = large). Defaults 0.
    pub body_size: u8,
    /// MNAM — menu-item text, one per entry. Order preserved. Each
    /// MNAM is flanked by its own sub-record group (NNAM target,
    /// CTDA conditions) which is deferred; the stub just captures
    /// the labels so the menu tree isn't lost.
    pub menu_items: Vec<String>,
}

pub fn parse_term(form_id: u32, subs: &[SubRecord]) -> TermRecord {
    let mut out = TermRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"MODL" => out.model_path = read_zstring(&sub.data),
            b"SCRI" => out.script_form_id = read_u32_at(&sub.data, 0).unwrap_or(0),
            b"ANAM" => out.password = read_zstring(&sub.data),
            b"DNAM" => out.footer_text = read_zstring(&sub.data),
            b"BSIZ" if !sub.data.is_empty() => {
                out.body_size = sub.data[0];
            }
            b"MNAM" => {
                // FO3/FNV sometimes ships MNAM as the menu-item text
                // directly and sometimes as a 4-byte form ref (when
                // the label lives elsewhere). Treat as text whenever
                // the bytes are printable; otherwise skip. Keeps the
                // stub robust against the mixed wild encoding.
                let text = read_zstring(&sub.data);
                if !text.is_empty() {
                    out.menu_items.push(text);
                }
            }
            _ => {}
        }
    }
    out
}

/// Actor Value Information record (`AVIF`). Defines the ~30 actor
/// values FO3/FNV expose to the perk / VATS / SPECIAL pipelines —
/// Strength, Endurance, CombatSkill, every governed skill, plus
/// resistances + resources. Skyrim+ adds a per-skill perk-tree
/// graph (PNAM/INAM/CNAM section list); only the FO3/FNV-shape
/// fields are captured here. The Skyrim perk-tree decoder lands
/// alongside the perk-graph consumer.
///
/// Pre-fix the whole top-level group fell through the catch-all
/// skip in `parse_esm`, so every NPC `skill_bonuses` cross-ref,
/// every BOOK skill-book ref, and every AVIF-keyed condition
/// predicate (~300 condition functions) dangled. See #519.
#[derive(Debug, Clone, Default)]
pub struct AvifRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// `DESC` — long description shown in the Pip-Boy / skills UI.
    pub description: String,
    /// `ANAM` — short-form abbreviation. Only present on a handful
    /// of values (1Hand, 2Hand, etc.); empty otherwise.
    pub abbreviation: String,
    /// `CNAM` — skill category for skill-typed AVIFs:
    /// 0 = None, 1 = Combat, 2 = Magic, 3 = Stealth.
    /// Non-skill AVIFs reuse the four bytes for opaque flag data
    /// (kept verbatim — semantics differ per game, decoded by the
    /// consuming subsystem).
    pub category: u32,
    /// `AVSK` — skill-scaling tuple (only present for skill AVIFs):
    /// `[skill_use_mult, skill_use_offset, skill_improve_mult, skill_improve_offset]`.
    /// `None` for non-skill records (resistances, resources, attributes).
    pub skill_scaling: Option<[f32; 4]>,
}

pub fn parse_avif(form_id: u32, subs: &[SubRecord]) -> AvifRecord {
    let mut out = AvifRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"DESC" => out.description = read_lstring_or_zstring(&sub.data),
            b"ANAM" => out.abbreviation = read_zstring(&sub.data),
            b"CNAM" => out.category = read_u32_at(&sub.data, 0).unwrap_or(0),
            b"AVSK" if sub.data.len() >= 16 => {
                out.skill_scaling = Some([
                    read_f32_at(&sub.data, 0).unwrap_or(0.0),
                    read_f32_at(&sub.data, 4).unwrap_or(0.0),
                    read_f32_at(&sub.data, 8).unwrap_or(0.0),
                    read_f32_at(&sub.data, 12).unwrap_or(0.0),
                ]);
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(typ: &[u8; 4], data: &[u8]) -> SubRecord {
        SubRecord {
            sub_type: *typ,
            data: data.to_vec(),
        }
    }

    #[test]
    fn parse_watr_picks_edid_full_tnam() {
        let subs = vec![
            sub(b"EDID", b"WaterFreshDefault\0"),
            sub(b"FULL", b"Fresh Water\0"),
            sub(b"TNAM", b"textures\\water\\fresh.dds\0"),
        ];
        let w = parse_watr(0x1234, &subs);
        assert_eq!(w.form_id, 0x1234);
        assert_eq!(w.editor_id, "WaterFreshDefault");
        assert_eq!(w.full_name, "Fresh Water");
        assert_eq!(w.texture_path, "textures\\water\\fresh.dds");
    }

    #[test]
    fn parse_navi_extracts_version() {
        let subs = vec![
            sub(b"EDID", b"NavMaster\0"),
            sub(b"NVER", &11u32.to_le_bytes()),
        ];
        let n = parse_navi(0x5678, &subs);
        assert_eq!(n.editor_id, "NavMaster");
        assert_eq!(n.version, 11);
    }

    #[test]
    fn parse_navm_extracts_version() {
        let subs = vec![sub(b"NVER", &11u32.to_le_bytes())];
        let n = parse_navm(0xAABB, &subs);
        assert_eq!(n.form_id, 0xAABB);
        assert_eq!(n.version, 11);
    }

    #[test]
    fn parse_regn_picks_weather_and_color() {
        let subs = vec![
            sub(b"EDID", b"WastelandRegion\0"),
            sub(b"WNAM", &0x0001_B000u32.to_le_bytes()),
            sub(b"RCLR", &[128, 96, 64, 0]),
        ];
        let r = parse_regn(0xBEEF, &subs);
        assert_eq!(r.editor_id, "WastelandRegion");
        assert_eq!(r.weather_form, Some(0x0001_B000));
        assert_eq!(r.color, Some([128, 96, 64]));
    }

    #[test]
    fn parse_eczn_picks_owner_rank_flags() {
        let mut data = Vec::new();
        data.extend_from_slice(&0x0001_CAFEu32.to_le_bytes()); // owner form
        data.push(3); // rank
        data.push(15); // min level
        data.push(0x02); // flags
        let subs = vec![sub(b"EDID", b"NcrZone\0"), sub(b"DATA", &data)];
        let z = parse_eczn(0x9876, &subs);
        assert_eq!(z.editor_id, "NcrZone");
        assert_eq!(z.owner_form, 0x0001_CAFE);
        assert_eq!(z.rank, 3);
        assert_eq!(z.min_level, 15);
        assert_eq!(z.flags, 0x02);
    }

    #[test]
    fn parse_lgtm_decodes_xcll_prefix() {
        // Use distinct byte patterns so an off-by-one on any field
        // surfaces as a visible assertion failure.
        let mut data = Vec::with_capacity(40);
        data.extend_from_slice(&[80, 82, 85, 0]); // ambient
        data.extend_from_slice(&[200, 195, 180, 0]); // directional
        data.extend_from_slice(&[40, 45, 50, 0]); // fog color
        data.extend_from_slice(&64.0f32.to_le_bytes()); // fog near
        data.extend_from_slice(&4000.0f32.to_le_bytes()); // fog far
        data.extend_from_slice(&0i32.to_le_bytes()); // rot X
        data.extend_from_slice(&0i32.to_le_bytes()); // rot Y
        data.extend_from_slice(&0.5f32.to_le_bytes()); // dir fade
        data.extend_from_slice(&6000.0f32.to_le_bytes()); // fog clip
        data.extend_from_slice(&1.25f32.to_le_bytes()); // fog power
        let subs = vec![sub(b"EDID", b"LgtmInteriorDim\0"), sub(b"DATA", &data)];
        let l = parse_lgtm(0xDEAD, &subs);
        assert_eq!(l.editor_id, "LgtmInteriorDim");
        assert!((l.ambient[0] - 80.0 / 255.0).abs() < 1e-6);
        assert!((l.directional[1] - 195.0 / 255.0).abs() < 1e-6);
        assert!((l.fog_color[2] - 50.0 / 255.0).abs() < 1e-6);
        assert_eq!(l.fog_near, 64.0);
        assert_eq!(l.fog_far, 4000.0);
        assert_eq!(l.directional_fade, Some(0.5));
        assert_eq!(l.fog_clip, Some(6000.0));
        assert_eq!(l.fog_power, Some(1.25));
    }

    #[test]
    fn parse_lgtm_short_data_returns_defaults() {
        // DATA under 20 bytes → all field captures short-circuit.
        let subs = vec![
            sub(b"EDID", b"ShortLgtm\0"),
            sub(b"DATA", &[1, 2, 3, 4]),
        ];
        let l = parse_lgtm(0xBEEF, &subs);
        assert_eq!(l.editor_id, "ShortLgtm");
        assert_eq!(l.ambient, [0.0; 3]);
        assert_eq!(l.fog_near, 0.0);
        assert!(l.directional_fade.is_none());
    }

    #[test]
    fn parse_hdpt_picks_edid_model_flags() {
        let subs = vec![
            sub(b"EDID", b"HumanHead01\0"),
            sub(b"FULL", b"Human Head\0"),
            sub(b"MODL", b"meshes\\characters\\head.nif\0"),
            sub(b"DATA", &[0x01]),
        ];
        let h = parse_hdpt(0x1111, &subs);
        assert_eq!(h.editor_id, "HumanHead01");
        assert_eq!(h.model_path, "meshes\\characters\\head.nif");
        assert_eq!(h.flags, 0x01);
    }

    #[test]
    fn parse_eyes_picks_icon_flags() {
        let subs = vec![
            sub(b"EDID", b"EyeBlue\0"),
            sub(b"FULL", b"Blue Eyes\0"),
            sub(b"ICON", b"textures\\characters\\eyes\\blue.dds\0"),
            sub(b"DATA", &[0x02]),
        ];
        let e = parse_eyes(0x2222, &subs);
        assert_eq!(e.icon_path, "textures\\characters\\eyes\\blue.dds");
        assert_eq!(e.flags, 0x02);
    }

    #[test]
    fn parse_hair_picks_model_icon_flags() {
        let subs = vec![
            sub(b"EDID", b"HairBrown01\0"),
            sub(b"FULL", b"Brown Hair\0"),
            sub(b"MODL", b"meshes\\characters\\hair\\brown.nif\0"),
            sub(b"ICON", b"textures\\characters\\hair\\brown.dds\0"),
            sub(b"DATA", &[0x00]),
        ];
        let h = parse_hair(0x3333, &subs);
        assert_eq!(h.model_path, "meshes\\characters\\hair\\brown.nif");
        assert_eq!(h.icon_path, "textures\\characters\\hair\\brown.dds");
        assert_eq!(h.flags, 0x00);
    }

    // ── AI / dialogue / effect stubs (#446, #447) ──────────────────

    #[test]
    fn parse_pack_picks_pkdt_flags_and_procedure() {
        let mut pkdt = Vec::new();
        pkdt.extend_from_slice(&0x0000_0421u32.to_le_bytes()); // flags
        pkdt.extend_from_slice(&6u32.to_le_bytes()); // procedure 6 = Patrol
        let subs = vec![sub(b"EDID", b"GuardPatrolDay\0"), sub(b"PKDT", &pkdt)];
        let p = parse_pack(0xA1A1, &subs);
        assert_eq!(p.editor_id, "GuardPatrolDay");
        assert_eq!(p.package_flags, 0x0000_0421);
        assert_eq!(p.procedure_type, 6);
    }

    #[test]
    fn parse_qust_picks_scri_and_data_flags() {
        let subs = vec![
            sub(b"EDID", b"MQ01\0"),
            sub(b"FULL", b"Main Quest\0"),
            sub(b"SCRI", &0x0010_BEEFu32.to_le_bytes()),
            sub(b"DATA", &[0x05, 20]), // flags + priority
        ];
        let q = parse_qust(0xB2B2, &subs);
        assert_eq!(q.editor_id, "MQ01");
        assert_eq!(q.full_name, "Main Quest");
        assert_eq!(q.script_ref, 0x0010_BEEF);
        assert_eq!(q.quest_flags, 0x05);
        assert_eq!(q.priority, 20);
    }

    #[test]
    fn parse_dial_accumulates_multiple_quest_refs() {
        let subs = vec![
            sub(b"EDID", b"GREETING\0"),
            sub(b"FULL", b"Greeting\0"),
            sub(b"QSTI", &0x0100_0001u32.to_le_bytes()),
            sub(b"QSTI", &0x0100_0002u32.to_le_bytes()),
            sub(b"QSTI", &0x0100_0003u32.to_le_bytes()),
        ];
        let d = parse_dial(0xC3C3, &subs);
        assert_eq!(d.quest_refs.len(), 3);
        assert_eq!(d.quest_refs[1], 0x0100_0002);
    }

    #[test]
    fn parse_mesg_picks_desc_and_owner_quest() {
        let subs = vec![
            sub(b"EDID", b"FastTravelMessage\0"),
            sub(b"FULL", b"Fast Travel\0"),
            sub(b"DESC", b"You cannot fast travel right now.\0"),
            sub(b"QNAM", &0x0002_1234u32.to_le_bytes()),
        ];
        let m = parse_mesg(0xD4D4, &subs);
        assert_eq!(m.description, "You cannot fast travel right now.");
        assert_eq!(m.owner_quest, 0x0002_1234);
    }

    #[test]
    fn parse_perk_picks_data_flags() {
        let subs = vec![
            sub(b"EDID", b"IntenseTraining\0"),
            sub(b"FULL", b"Intense Training\0"),
            sub(b"DESC", b"Increase any one S.P.E.C.I.A.L. by 1.\0"),
            sub(b"DATA", &[0x01]), // playable
        ];
        let p = parse_perk(0xE5E5, &subs);
        assert_eq!(p.editor_id, "IntenseTraining");
        assert_eq!(p.perk_flags, 0x01);
    }

    #[test]
    fn parse_spel_picks_spit_cost_and_flags() {
        let mut spit = Vec::new();
        spit.extend_from_slice(&42u32.to_le_bytes()); // cost
        spit.extend_from_slice(&[0u8; 8]); // padding to flags offset
        spit.extend_from_slice(&0x0000_0004u32.to_le_bytes()); // flags
        let subs = vec![sub(b"EDID", b"Fireball\0"), sub(b"SPIT", &spit)];
        let s = parse_spel(0xF6F6, &subs);
        assert_eq!(s.cost, 42);
        assert_eq!(s.spell_flags, 0x0000_0004);
    }

    #[test]
    fn parse_mgef_picks_data_effect_flags() {
        let subs = vec![
            sub(b"EDID", b"RadiationPoisoning\0"),
            sub(b"FULL", b"Radiation Poisoning\0"),
            sub(b"DESC", b"Contaminated by radiation.\0"),
            sub(b"DATA", &0x0000_0009u32.to_le_bytes()),
        ];
        let e = parse_mgef(0xA7A7, &subs);
        assert_eq!(e.effect_flags, 0x0000_0009);
    }

    #[test]
    fn parse_acti_extracts_scri_and_model() {
        let subs = vec![
            sub(b"EDID", b"NukaColaMachine01\0"),
            sub(b"FULL", b"Nuka-Cola Machine\0"),
            sub(b"MODL", b"activators\\nukacolamachine01.nif\0"),
            sub(b"SCRI", &0x0010_ABCDu32.to_le_bytes()),
            sub(b"SNAM", &0x0009_0000u32.to_le_bytes()),
        ];
        let a = parse_acti(0x0002_9E7A, &subs);
        assert_eq!(a.editor_id, "NukaColaMachine01");
        assert_eq!(a.full_name, "Nuka-Cola Machine");
        assert_eq!(a.model_path, "activators\\nukacolamachine01.nif");
        assert_eq!(a.script_form_id, 0x0010_ABCD);
        assert_eq!(a.sound_form_id, 0x0009_0000);
        // Radio form defaults to 0 when RNAM/RADR absent.
        assert_eq!(a.radio_form_id, 0);
    }

    #[test]
    fn parse_term_extracts_password_footer_menu() {
        let subs = vec![
            sub(b"EDID", b"Vault21OverseerTerminal\0"),
            sub(b"FULL", b"Overseer's Terminal\0"),
            sub(b"MODL", b"clutter\\junk\\terminal01.nif\0"),
            sub(b"ANAM", b"tranquility\0"),
            sub(b"DNAM", b"Welcome, Overseer. Vault 21 online.\0"),
            sub(b"BSIZ", &[1u8]),
            sub(b"MNAM", b"Open Vault Door\0"),
            sub(b"MNAM", b"View Resident Log\0"),
            sub(b"MNAM", b"Disable Security\0"),
            sub(b"SCRI", &0x0004_2CD2u32.to_le_bytes()),
        ];
        let t = parse_term(0x0004_2424, &subs);
        assert_eq!(t.editor_id, "Vault21OverseerTerminal");
        assert_eq!(t.password, "tranquility");
        assert!(t.footer_text.starts_with("Welcome, Overseer"));
        assert_eq!(t.body_size, 1);
        assert_eq!(t.menu_items.len(), 3);
        assert_eq!(t.menu_items[0], "Open Vault Door");
        assert_eq!(t.menu_items[2], "Disable Security");
        assert_eq!(t.script_form_id, 0x0004_2CD2);
    }

    #[test]
    fn parse_avif_skill_record_decodes_avsk_and_category() {
        // Small Guns: skill, Combat category, full AVSK tuple.
        let mut avsk = Vec::new();
        avsk.extend_from_slice(&1.0f32.to_le_bytes()); // skill_use_mult
        avsk.extend_from_slice(&0.0f32.to_le_bytes()); // skill_use_offset
        avsk.extend_from_slice(&1.5f32.to_le_bytes()); // skill_improve_mult
        avsk.extend_from_slice(&2.0f32.to_le_bytes()); // skill_improve_offset
        let subs = vec![
            sub(b"EDID", b"SmallGuns\0"),
            sub(b"FULL", b"Small Guns\0"),
            sub(b"DESC", b"Affects accuracy with pistols and rifles.\0"),
            sub(b"ANAM", b"SG\0"),
            sub(b"CNAM", &1u32.to_le_bytes()), // Combat
            sub(b"AVSK", &avsk),
        ];
        let a = parse_avif(0x0000_002B, &subs);
        assert_eq!(a.editor_id, "SmallGuns");
        assert_eq!(a.full_name, "Small Guns");
        assert_eq!(a.abbreviation, "SG");
        assert_eq!(a.category, 1);
        let scaling = a.skill_scaling.expect("AVSK populated for skill records");
        assert_eq!(scaling, [1.0, 0.0, 1.5, 2.0]);
    }

    #[test]
    fn parse_avif_non_skill_record_has_no_avsk() {
        // Strength: SPECIAL attribute — no AVSK, no category set.
        let subs = vec![
            sub(b"EDID", b"Strength\0"),
            sub(b"FULL", b"Strength\0"),
            sub(b"DESC", b"Raw physical power.\0"),
        ];
        let a = parse_avif(0x0000_0000, &subs);
        assert_eq!(a.editor_id, "Strength");
        assert_eq!(a.category, 0);
        assert!(a.skill_scaling.is_none());
    }

    #[test]
    fn parse_term_unlocked_terminal_has_empty_password() {
        // Tutorial / ambient terminals often ship without ANAM; stub
        // must tolerate that without panicking.
        let subs = vec![
            sub(b"EDID", b"GoodspringsSchoolTerminal\0"),
            sub(b"FULL", b"School Terminal\0"),
            sub(b"DNAM", b"Primer by Mr. Goodsprings.\0"),
        ];
        let t = parse_term(0x0008_1111, &subs);
        assert!(t.password.is_empty());
        assert_eq!(t.body_size, 0);
        assert!(t.menu_items.is_empty());
    }
}
