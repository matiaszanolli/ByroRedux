//! `ACTI` activators and `TERM` terminals.


use byroredux_core::bytes::le_u32_at;
use super::super::super::common::{read_lstring_field, read_lstring_or_zstring, remap_fid, CommonNamedFields};
use super::super::super::condition::{push_ctda, ConditionList};
use crate::esm::reader::{FormIdRemap, SubRecord};
use crate::esm::sub_reader::SubReader;

/// `ACTI` activator record. FO3/FNV/Oblivion wall switches, buttons,
/// vending machines, lever-activated doors — anything that the player
/// "use"s but isn't a container, door, or NPC. SCRI on these records
/// runs the trigger script; DEST controls destruction-state meshes.
/// Full destruction-stage decoding is deferred — the stub captures
/// identity + model + SCRI cross-ref so dangling references resolve
/// at lookup time. See #521.
///
/// **`script_form_id` is live since M47.0**: `ActiRecord` is the first arm
/// of `EsmIndex::base_record_script` (`records/index.rs`), which
/// `byroredux/src/cell_loader/references/attach.rs` calls to resolve
/// `index.scripts.get(&script_form_id)` and dispatch `ActivateEvent` to the
/// SCRI-linked script (the attach path logs with an `"M47.0: "` prefix).
///
/// **Runtime consumer gap, still open:** `sound_form_id` / `radio_form_id`
/// cross-refs ride through unused today — no reader outside `records/`
/// plays the SNAM/RNAM sound on `OnActivate` yet. The stub closes the
/// parser-side silent drop so that work has a grep target.
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
    /// script. Live since M47.0 via `EsmIndex::base_record_script` +
    /// `cell_loader::references::attach`, which dispatches `ActivateEvent`
    /// to the resolved script.
    pub script_form_id: u32,
    /// SNAM — sound form ID played on activation (optional).
    pub sound_form_id: u32,
    /// RADR / RNAM — radio station form ID, applicable to FNV radio
    /// transmitters (activator variant). `0` when absent.
    pub radio_form_id: u32,
    /// Decoded `VMAD` script attachments (Skyrim+ inline Papyrus). `None`
    /// on FO3 / FNV / Oblivion activators (those use the `SCRI` →  SCPT /
    /// Obscript path). Consumed by the M47.2 scripting-translation layer
    /// to fetch + decompile the attached `.pex`. Activators are the most
    /// common scripted base record in Skyrim cells (levers, doors, traps).
    pub script_instance: Option<crate::esm::records::script_instance::ScriptInstanceData>,
    /// Skyrim+ `WNAM` — the WATR record a placed water activator renders and
    /// behaves as. Vanilla Skyrim authors its sloped river and stream
    /// sections this way (`Water1024RiverFlowNE`, `TundraStreamStraight01WaterA`
    /// → `CreekWaterFlow`): a `BSWaterShaderProperty` mesh whose colours,
    /// current and noise come from this water type rather than the NIF.
    /// `0` when absent. Consumers must resolve it against the WATR table.
    pub water_type_form: u32,
}

pub fn parse_acti(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> ActiRecord {
    // EDID / FULL / MODL / SCRI / VMAD via the shared helper (TD2-109 /
    // #2068). VMAD is the Skyrim+ inline Papyrus attachment — absent on
    // FO3/FNV, which carry SCRI instead — and the helper decodes it so the
    // M47.2 attach path can decompile the named `.pex` and bind its
    // properties. Only the ACTI-specific sound / radio arms remain below.
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    let mut out = ActiRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        model_path: common.model_path,
        script_form_id: common.script_form_id,
        script_instance: common.script_instance,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            // #3401 — `parse_acti` already receives the remap and applies
            // it to the shared named fields; these two read FormIDs a
            // dozen lines later and used to skip it.
            b"SNAM" => {
                out.sound_form_id = remap_fid(SubReader::new(&sub.data).u32_or_default(), remap);
            }
            b"RNAM" | b"RADR" => {
                out.radio_form_id = remap_fid(SubReader::new(&sub.data).u32_or_default(), remap);
            }
            b"WNAM" => {
                out.water_type_form = remap_fid(SubReader::new(&sub.data).u32_or_default(), remap);
            }
            _ => {}
        }
    }
    out
}

/// One entry in a `TERM` terminal's menu.
///
/// #4171 — the shipped layout is a repeating sub-record group, not the
/// flat `Vec<String>` this used to be flattened into. Measured over the
/// masters (2026-09-12), the group is `ITXT [RNAM] ANAM [ITID] [UNAM]`
/// and the ordering is exact: every one of FNV's 895 `RNAM` and FO4's 545
/// is immediately preceded by an `ITXT` and immediately followed by an
/// `ANAM`. On Fallout 4 the record's `ISIZ` equals the `ITXT` count for
/// all 778 terminals, which is what pins the grouping rather than leaving
/// it inferred.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TerminalMenuItem {
    /// `ITXT` — the menu line as displayed. Inline zstring on FO3/FNV
    /// (`"Dismiss all companions."`), a 4-byte lstring id on FO4.
    pub text: String,
    /// `RNAM` — the text shown after the entry is selected
    /// (`"Accessing confirmation menu..."`). Always present on FNV,
    /// optional on FO4 (545 of 2 227 entries); empty when absent.
    pub result_text: String,
    /// `ITID` — the menu-item id the FO4 terminal-menu script switches
    /// on. FO4-only; 0 on FO3/FNV, which author no ITID.
    pub item_id: u16,
    /// `ANAM` — one byte, present once per menu item on both FO3/FNV and
    /// FO4. The individual bit meanings are **unsourced**: no citable
    /// layout was found for them, and the observed values are not
    /// self-explanatory. Captured verbatim rather than guessed at.
    pub flags: u8,
    /// #4641 — `UNAM` — the "Display Text" an `ANAM = 8` menu item
    /// actually shows in-game, distinct from [`Self::text`]. 1 804 of
    /// FO4's 1 818 `ANAM = 8` items carry their text here; FO3/FNV
    /// author no UNAM, leaving this empty.
    pub display_text: String,
    /// #4641 — `CTDA`/`CTDT` conditions authored inside this item's
    /// sub-record group (the xEdit FO4 `TERM` menu-item struct ends in a
    /// conditions array). Empty on FO3/FNV.
    pub conditions: ConditionList,
}

/// One `BTXT`/`DESC` body block, with the conditions that gate it.
///
/// #4641 — xEdit's FO4 `TERM` lays the body out as a `BSIZ`-counted
/// array of `{BTXT, Conditions}` structs: 82 `Fallout4.esm` terminals
/// author 2–7 conditional bodies. The pre-fix single `body_text` field
/// kept only the last `BTXT` and discarded every condition. FO3/FNV's
/// `DESC` opens a body through the same struct, giving one shape for
/// both eras.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TerminalBody {
    /// `BTXT` (FO4) or `DESC` (FO3/FNV) — this body's text, routed
    /// through the lstring reader.
    pub text: String,
    /// Conditions authored between this body and the next structural
    /// opener (a following `BTXT`/`DESC` or the first `ITXT`).
    pub conditions: ConditionList,
}

/// `TERM` terminal record — FO3/FNV computer consoles. Carries a
/// menu tree (MNAM entries), password (ANAM), body text (DNAM), and
/// the NIF model of the physical terminal. MNAM text is collected
/// into `menu_items` so a future terminal-interaction system can
/// walk the options without re-parsing. See #521.
///
/// **SCRI is live since M47.0**: `TermRecord` is one of the
/// `EsmIndex::base_record_script` arms (`records/index.rs`), so
/// `cell_loader::references::attach` resolves and dispatches a hacked
/// terminal's script the same generic way it does for `ACTI`.
///
/// **Runtime consumer gap, still open:** the menu tree and password ride
/// through unused — terminal interaction needs the event-hook runtime for
/// NNAM target dispatch + CTDA option-gate evaluation, plus a UI overlay
/// for the `body_size`-driven screen. The stub captures the surface so
/// that work has one grep target and the labels don't have to be
/// re-walked from the ESM.
#[derive(Debug, Clone, Default)]
pub struct TermRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub model_path: String,
    /// SCRI — script form ID (some terminals trigger quest advance
    /// scripts on successful hack). Live since M47.0 via
    /// `EsmIndex::base_record_script` + `cell_loader::references::attach`.
    pub script_form_id: u32,
    /// #4641 — `NAM0` — the terminal's header text (lstring). Authored
    /// by 291 of FO4's 778 terminals; absent (empty) on FO3/FNV and the
    /// rest.
    pub header_text: String,
    /// #4641 — `WNAM` — the welcome text shown on first access
    /// (lstring). Authored by 467 of FO4's 778 terminals; absent
    /// (empty) on FO3/FNV.
    pub welcome_text: String,
    /// Every authored body block — `DESC` on FO3/FNV, the `BSIZ`-counted
    /// `{BTXT, Conditions}` array on FO4 — in authored order (#4641).
    /// Pre-fix this collapsed to a single last-wins `BTXT`.
    pub bodies: Vec<TerminalBody>,
    /// The terminal's displayed body text — every [`Self::bodies`]
    /// entry's `text`, joined with `"\n"` (#4641). Single-body records
    /// (all of FO3/FNV, 696 of 778 FO4) read exactly as before.
    pub body_text: String,
    /// BSIZ — body-text size. FO4-only and a **u32**, not the u8 this
    /// field used to be: all 778 `Fallout4.esm` terminals ship a 4-byte
    /// payload. FO3/FNV author no BSIZ at all, so this stays 0 there.
    pub body_size: u32,
    /// The terminal's menu, in authored order. See [`TerminalMenuItem`].
    pub menu_items: Vec<TerminalMenuItem>,
    /// Decoded `VMAD` script attachments (Skyrim+ inline Papyrus). `None`
    /// on FO3/FNV terminals (those use the `SCRI` → SCPT/Obscript path
    /// via `script_form_id`). #2663 (SCR-D7-NEW11-02) — FO4 ships 207
    /// VMAD-bearing `TERM` records (`Fallout4.esm`); the shared helper
    /// already decoded this, `parse_term` just wasn't copying it out.
    pub script_instance: Option<super::super::super::script_instance::ScriptInstanceData>,
    /// M47.2 — the `Fragment_Terminal_NN` bindings of a Fallout 4 / Starfield
    /// terminal's `VMAD` ([`parse_indexed_fragments`]), keyed by menu item id.
    pub script_fragments: Vec<super::super::super::script_instance::IndexedScriptFragment>,
}

pub fn parse_term(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> TermRecord {
    // EDID / FULL / MODL / SCRI / VMAD via the shared helper (TD2-109 /
    // #2068). TERM is NOT FO3/FNV-only — FO4 ships 207 VMAD-bearing TERM
    // records — so the helper's VMAD arm fires there; #2663 fixed
    // `parse_term` dropping the decoded `script_instance` on the floor.
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    let mut out = TermRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        model_path: common.model_path,
        script_form_id: common.script_form_id,
        script_instance: common.script_instance,
        ..Default::default()
    };
    // #4171 — this loop used to read `password` from ANAM, `footer_text`
    // from DNAM and `menu_items` from MNAM. None of the three is authored
    // text on any shipped game; the census that settled it (2026-09-12,
    // `FalloutNV.esm` 344 records / `Fallout4.esm` 778):
    //
    //   ANAM  1 byte, once per menu item (FNV 895, FO4 2 227) — a flag
    //         byte, never a password. `read_zstring` on it produced a
    //         1-character string from whatever the flags happened to be.
    //   DNAM  FNV-only, 4 bytes, 342 of 344 (a packed field, top values
    //         `00 02 04 00` / `00 02 00 00`). FO4 authors none, so
    //         `footer_text` was empty there and garbage on FNV.
    //   MNAM  FO4-only, 4 bytes, value `01 00 00 40` on all 778 — a
    //         float, not text. FNV ships none at all, so `menu_items`
    //         was always empty on FNV and always 778 one-character
    //         entries on FO4.
    //
    // The real text is ITXT / RNAM / BTXT / DESC, none of which had a
    // match arm anywhere in the crate. DNAM and MNAM are deliberately
    // left unread: their payloads are measured but their field layouts
    // are unsourced, and inventing a decode is what produced this bug the
    // first time.
    //
    // #4641 — the structural openers are `BTXT`/`DESC` (bodies) and
    // `ITXT` (menu items). A `CTDA` attaches to whichever element opened
    // most recently — the xEdit FO4 layout ends both the body struct and
    // the menu-item struct with a conditions array.
    if let Some(vmad) = subs.iter().find(|sub| sub.sub_type == *b"VMAD") {
        out.script_fragments = super::super::super::script_instance::parse_indexed_fragments(&vmad.data);
    }
    let mut current_body: Option<TerminalBody> = None;
    // `true` when the last structural opener was a menu item, `false`
    // when it was a body, `None` before any opener.
    let mut in_menu_item = None::<bool>;
    for sub in subs {
        match &sub.sub_type {
            // Body openers: DESC on FO3/FNV, BTXT on FO4. No shipped
            // record carries both, and #4641 models the FO4 body as a
            // BSIZ-counted array — each opener starts a fresh body.
            b"DESC" | b"BTXT" => {
                if let Some(finished) = current_body.take() {
                    out.bodies.push(finished);
                }
                current_body = Some(TerminalBody {
                    text: read_lstring_field(b"TERM", &sub.sub_type, &sub.data),
                    conditions: ConditionList::default(),
                });
                in_menu_item = Some(false);
            }
            // #4641 — header / welcome text (lstrings), FO4-only.
            b"NAM0" => out.header_text = read_lstring_or_zstring(&sub.data),
            b"WNAM" => out.welcome_text = read_lstring_or_zstring(&sub.data),
            b"BSIZ" => {
                out.body_size = match sub.data.len() {
                    4 => le_u32_at(&sub.data, 0).unwrap_or_default(),
                    1..=3 => SubReader::new(&sub.data).u8_or_default() as u32,
                    _ => 0,
                };
            }
            // ITXT opens a new menu item; RNAM / ANAM / ITID / UNAM
            // attach to the one it opened. Bare RNAM/ANAM before any ITXT
            // cannot occur in shipped content and is dropped rather than
            // synthesizing a textless item.
            b"ITXT" => {
                if let Some(finished) = current_body.take() {
                    out.bodies.push(finished);
                }
                out.menu_items.push(TerminalMenuItem {
                    text: read_lstring_or_zstring(&sub.data),
                    ..Default::default()
                });
                in_menu_item = Some(true);
            }
            b"RNAM" => {
                if let Some(item) = out.menu_items.last_mut() {
                    item.result_text = read_lstring_or_zstring(&sub.data);
                }
            }
            b"ANAM" if sub.data.len() == 1 => {
                if let Some(item) = out.menu_items.last_mut() {
                    item.flags = sub.data[0];
                }
            }
            b"ITID" => {
                if let (Ok(id), Some(item)) = (SubReader::new(&sub.data).u16(), out.menu_items.last_mut()) {
                    item.item_id = id;
                }
            }
            // #4641 — `UNAM` — the text an `ANAM = 8` ("Display Text")
            // menu item actually shows in-game (1 804 of 1 818 such FO4
            // items carry it).
            b"UNAM" => {
                if let Some(item) = out.menu_items.last_mut() {
                    item.display_text = read_lstring_or_zstring(&sub.data);
                }
            }
            // #4641 — conditions attach to the most recently opened
            // element (body or menu item), matching the xEdit FO4
            // struct layout. `CTDT` is the legacy encoding; see
            // `push_ctda`'s doc.
            b"CTDA" | b"CTDT" | b"CIS1" | b"CIS2" => match (in_menu_item, &mut current_body) {
                (Some(false), Some(body)) => push_ctda(sub, remap, &mut body.conditions),
                (Some(true), _) => {
                    if let Some(item) = out.menu_items.last_mut() {
                        push_ctda(sub, remap, &mut item.conditions);
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    if let Some(finished) = current_body.take() {
        out.bodies.push(finished);
    }
    // #4641 — derive the flat convenience field from the full sequence,
    // the same shape `parse_info` uses for its multi-segment texts.
    out.body_text = out
        .bodies
        .iter()
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    out
}
