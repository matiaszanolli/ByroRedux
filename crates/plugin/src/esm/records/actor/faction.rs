use super::super::common::{read_zstring, remap_fid, CommonNamedFields};
use crate::esm::reader::{FormIdRemap, SubRecord};
use crate::esm::sub_reader::SubReader;

/// Shortest `CRVA` the parser decodes: Skyrim lets the record end after the
/// `unknown` word (14 bytes; the `actor/tests.rs` short-CRVA case truncates
/// there), so the steal multiplier / escape / werewolf tail is optional.
const CRVA_MIN_LEN: usize = 14;
/// `CRVA` length at which the optional tail (steal multiplier f32, escape u16,
/// werewolf u16) is fully present.
const CRVA_TAIL_LEN: usize = 20;

/// Faction-to-faction relation.
#[derive(Debug, Clone, Copy)]
pub struct FactionRelation {
    pub other_faction: u32,
    /// Modifier (-100..100, larger means more friendly).
    pub modifier: i32,
    /// Combat reaction (0=neutral, 1=enemy, 2=ally, 3=friend).
    ///
    /// Stored at the on-disk width (`u32` per UESP), not narrowed to the
    /// vanilla 0..=3 range. Pre-#3339 this was `u8` while the parser read
    /// a full `u32` and cast — so the `as u8` truncated back to exactly the
    /// eight bits the pre-#482 `sub.data[8]` read had, defeating the stated
    /// purpose of the wider read. Vanilla FNV never exceeds 3 (all 1,314
    /// `XNAM` sub-records are 12 bytes; values `{0: 179, 1: 264, 2: 472,
    /// 3: 399}`), so no live data changes — this only makes the field
    /// honour what the parser actually decodes.
    pub combat_reaction: u32,
}

/// One rung of a faction's rank ladder (`RNAM` + its optional `MNAM` / `FNAM`).
///
/// The on-disk layout is a flat run of sub-records in which `RNAM` opens a rank
/// block and the title sub-records that follow belong to it, so a rank may
/// carry no title at all — vanilla FNV authors 111 `RNAM` against only 94
/// `MNAM` and 53 `FNAM`. Rank numbers are also not dense: they are whatever the
/// author typed, which is why [`Self::index`] is stored rather than implied by
/// position. See #3338.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FactionRank {
    /// The authored rank number from `RNAM`. `XRNK` (REFR ownership rank) and
    /// FACT membership ranks are expressed in these numbers, not in ladder
    /// position, so this is the field a lookup keys off.
    pub index: u32,
    /// Male rank title from `MNAM`. Empty when the rank authors none — 17 FNV
    /// ranks (`OmertaFaction` rank 0, `NCRCFPowderGangerFaction` rank 0, …)
    /// are title-less, which is legal and must not shift its neighbours.
    pub male: String,
    /// Female rank title from `FNAM`. Empty when absent; fewer than half of
    /// FNV's titled ranks author one, and the male title is the fallback.
    pub female: String,
}

impl FactionRank {
    /// The title to show for an actor of the given gender, falling back to the
    /// other gender's title when only one is authored (the vanilla shape — 53
    /// `FNAM` against 94 `MNAM`), and to `None` when the rank is untitled.
    pub fn title(&self, female: bool) -> Option<&str> {
        let (first, second) = if female {
            (&self.female, &self.male)
        } else {
            (&self.male, &self.female)
        };
        [first, second]
            .into_iter()
            .find(|s| !s.is_empty())
            .map(String::as_str)
    }
}

/// A vendor faction's merchant data (Skyrim / FO4 `VEND` / `VENC` / `VENV`,
/// xEdit `wbDefinitionsTES5` / `wbDefinitionsFO4`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VendorData {
    /// `VEND` — the "Vendor Buy/Sell List" `FLST`, 0 when absent.
    pub buy_sell_list: u32,
    /// `VENC` — the "Merchant Container" `REFR`, 0 when absent.
    pub container: u32,
    /// `VENV` "Start Hour" / "End Hour" / "Radius".
    pub start_hour: u16,
    pub end_hour: u16,
    pub radius: u16,
    /// `VENV` "Only Buys Stolen Items" (Skyrim) / "Buys Stolen Items" (FO4).
    pub buys_stolen: bool,
    /// `VENV` "Not/Sell Buy" (Skyrim) / "Buy/Sell Everything Not In List?"
    /// (FO4): the list names what the vendor does *not* trade.
    pub everything_not_in_list: bool,
}

/// `FACT` `DATA` flag bits (Skyrim / FO4, xEdit `wbDefinitionsTES5` / `FO4`) the crime tab uses.
pub mod crime_flags {
    pub const TRACK_CRIME: u32 = 1 << 6;
    pub const IGNORE_MURDER: u32 = 1 << 7;
    pub const IGNORE_ASSAULT: u32 = 1 << 8;
    pub const IGNORE_STEALING: u32 = 1 << 9;
    pub const IGNORE_TRESPASS: u32 = 1 << 10;
    pub const DO_NOT_REPORT_CRIMES_AGAINST_MEMBERS: u32 = 1 << 11;
    pub const USE_DEFAULT_CRIME_GOLD: u32 = 1 << 12;
    pub const IGNORE_PICKPOCKET: u32 = 1 << 13;
}

/// The Crime tab of a Skyrim / FO4 faction: whether it tracks crime, which crimes it ignores, what
/// each is fined, and where its jail and evidence chests are (CK *Faction*, *Creating a Custom
/// Crime Faction*). A faction with no such data has `None` on [`FactionRecord::crime`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FactionCrime {
    /// `DATA` flags (see [`crime_flags`]).
    pub flags: u32,
    /// `JAIL` "Exterior Jail Marker": where the player is released, a `PrisonMarker` reference.
    pub exterior_jail_marker: u32,
    /// `WAIT` "Follower Wait Marker".
    pub follower_wait_marker: u32,
    /// `STOL` "Stolen Goods Container": an evidence chest reference.
    pub stolen_goods_container: u32,
    /// `PLCN` "Player Inventory Container": holds the player's gear in jail.
    pub player_inventory_container: u32,
    /// `CRGR` "Shared Crime Faction List".
    pub shared_crime_factions: u32,
    /// `JOUT` "Jail Outfit".
    pub jail_outfit: u32,
    /// `CRVA` "Crime Values", when authored.
    pub values: Option<CrimeValues>,
}

/// `CRVA`: what the faction does about, and fines for, each crime (used when `USE_DEFAULT_CRIME_GOLD`
/// is not set; "Arrest" and "Attack On Sight" apply either way).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CrimeValues {
    pub arrest: bool,
    pub attack_on_sight: bool,
    pub murder: u16,
    pub assault: u16,
    pub trespass: u16,
    pub pickpocket: u16,
    pub steal_multiplier: f32,
    pub escape: u16,
    pub werewolf: u16,
}

#[derive(Debug, Clone)]
pub struct FactionRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// Hidden flag etc. (from DATA).
    pub flags: u32,
    pub relations: Vec<FactionRelation>,
    /// The faction's rank ladder, one entry per authored `RNAM`.
    ///
    /// **Not** positionally indexable — see [`FactionRank::index`]. #3338:
    /// this used to be a flat `Vec<String>` pushed from `MNAM` arrival order
    /// with `RNAM` ignored entirely, which is only correct for a faction whose
    /// ranks are numbered `0..n` *and* all titled. 17 of FNV's 682 factions
    /// break that: `OmertaFaction` authors rank 0 untitled then ranks 1 and 2
    /// titled, so `ranks[0]` returned rank 1's label — an off-by-one on every
    /// member of that set. `FNAM` (the female title) was discarded outright.
    pub ranks: Vec<FactionRank>,
    /// `REPU` FormID this faction's standing moves, from `WMI1` (#3325).
    ///
    /// FNV replaces FO3's single global karma with per-faction reputation,
    /// and this sub-record is the **only** edge from a faction to the meter
    /// it moves — without it `EsmIndex::reputations` is an orphan map that no
    /// runtime can key off, so vendor pricing, disguise reactions, quest
    /// branching and hostile/idolized greetings have no input.
    ///
    /// Byte-proven rather than assumed: all 46 `FACT` `WMI1` payloads in
    /// `FalloutNV.esm` resolve to a real `REPU` record against a whole-file
    /// FormID→type map (100%, alongside 36 more on `REFR`). `None` on every
    /// other game — no other title in the corpus authors `WMI1`.
    pub reputation: Option<u32>,
    /// Merchant data, `None` for a faction that authors none of `VEND` /
    /// `VENC` / `VENV`.
    pub vendor: Option<VendorData>,
    /// The Crime tab, on the games that author one (Skyrim, FO4); `None` elsewhere.
    pub crime: Option<FactionCrime>,
}

// ── Parsers ───────────────────────────────────────────────────────────

pub fn parse_fact(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> FactionRecord {
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    let mut record = FactionRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        flags: 0,
        relations: Vec::new(),
        ranks: Vec::new(),
        reputation: None,
        vendor: None,
        crime: None,
    };

    for sub in subs {
        match &sub.sub_type {
            // DATA (FNV FACT): flags is a single byte per UESP
            // `Mod_File_Format/FACT` (FO3 / FNV). The tail is a
            // variable-width payload (FNV adds `u8 unknown + f32 crime
            // gold multiplier`) that different vanilla records truncate
            // differently — reading 4 bytes pulled padding / neighbor
            // bytes into the high 24 bits, producing spurious bits 8+.
            // Only bits 0 (hidden from PC), 1 (evil), 2 (special
            // combat) are authoritative on FO3 / FNV.
            //
            // Skyrim and FO4 extend DATA to a full u32; if / when those
            // parse paths get added here, split per `GameKind`. See
            // #481 / FNV-2-L1.
            b"DATA" if !sub.data.is_empty() => {
                record.flags = sub.data[0] as u32;
            }
            // XNAM: relation entry — other faction (u32) + modifier (i32) + reaction (u32).
            // The reaction field is a full 4-byte u32 per UESP; pre-#482 the
            // parser read only the low byte via `sub.data[8]`, which happened
            // to be correct for vanilla values 0..=3 but would silently
            // truncate any future mod that extends the enum past 255. The
            // decoded value is stored at its full width (#3339) — the old
            // `as u8` cast here threw away the extra 24 bits immediately,
            // leaving the wider read purely a cursor-alignment step.
            b"XNAM" if sub.data.len() >= 8 => {
                let mut r = SubReader::new(&sub.data);
                let other = r.u32_or_default();
                let modifier = r.i32_or_default();
                let combat = if sub.data.len() >= 12 {
                    r.u32_or_default()
                } else {
                    0
                };
                record.relations.push(FactionRelation {
                    // #3714 SIBLING — the related faction's FormID.
                    // `index.factions` is global-keyed, and the sibling
                    // `WMI1` arm right below already remaps for exactly this
                    // reason (#3325).
                    other_faction: remap_fid(other, remap),
                    modifier,
                    combat_reaction: combat,
                });
            }
            // WMI1 (FNV): the faction's `REPU` FormID — the faction →
            // reputation edge (#3325). Remapped to global load-order space
            // like every other embedded FormID, so `index.reputations`
            // lookups (keyed by the remapped record header FormID) hit.
            b"VEND" if sub.data.len() >= 4 => {
                let list = remap_fid(SubReader::new(&sub.data).u32_or_default(), remap);
                record.vendor.get_or_insert_with(VendorData::default).buy_sell_list = list;
            }
            b"VENC" if sub.data.len() >= 4 => {
                let container = remap_fid(SubReader::new(&sub.data).u32_or_default(), remap);
                record.vendor.get_or_insert_with(VendorData::default).container = container;
            }
            // VENV: u16 start hour, u16 end hour, u16 radius, 2 unknown
            // bytes, u8 buys stolen, u8 not-in-list; FO4's following "Buys
            // NonStolen Items" byte is Skyrim's unknown padding, and the
            // parser does not know the game, so it is not read.
            b"VENV" if sub.data.len() >= 10 => {
                let mut r = SubReader::new(&sub.data);
                let vendor = record.vendor.get_or_insert_with(VendorData::default);
                vendor.start_hour = r.u16_or_default();
                vendor.end_hour = r.u16_or_default();
                vendor.radius = r.u16_or_default();
                vendor.buys_stolen = sub.data[8] != 0;
                vendor.everything_not_in_list = sub.data[9] != 0;
            }
            b"WMI1" if sub.data.len() >= 4 => {
                let raw = SubReader::new(&sub.data).u32_or_default();
                record.reputation = (raw != 0).then(|| remap_fid(raw, remap));
            }
            // RNAM opens a rank block; the MNAM / FNAM that follow belong to
            // it. #3338 — before this arm existed the parser pushed one entry
            // per MNAM and dropped RNAM, so a faction with an untitled rank
            // (or a non-dense ladder) had every later rank shifted down. The
            // width is a 4-byte rank number per UESP `Mod_File_Format/FACT`.
            b"RNAM" if sub.data.len() >= 4 => {
                record.ranks.push(FactionRank {
                    index: SubReader::new(&sub.data).u32_or_default(),
                    ..Default::default()
                });
            }
            // MNAM / FNAM: male / female rank label for the rank the preceding
            // RNAM opened. A title with no preceding RNAM opens an implicit
            // rank numbered by ladder position — Oblivion-era FACT records
            // (and the synthetic fixtures that predate #3338) author titles
            // without rank numbers, and dropping them would be a regression on
            // the very shape this parser has always handled.
            b"MNAM" | b"FNAM" => {
                let male = sub.sub_type == *b"MNAM";
                // A repeated title of the same gender means the author opened a
                // new rung without an RNAM, so start one rather than
                // overwriting — no authored title is ever dropped.
                let needs_new_rank = match record.ranks.last() {
                    None => true,
                    Some(rank) => !(if male { &rank.male } else { &rank.female }).is_empty(),
                };
                if needs_new_rank {
                    let index = record.ranks.len() as u32;
                    record.ranks.push(FactionRank {
                        index,
                        ..Default::default()
                    });
                }
                let rank = record.ranks.last_mut().expect("non-empty by construction");
                let label = read_zstring(&sub.data);
                if male {
                    rank.male = label;
                } else {
                    rank.female = label;
                }
            }
            _ => {}
        }
    }

    record
}

/// The Crime tab of a Skyrim / FO4 `FACT`. Split from [`parse_fact`] because the game decides the
/// `DATA` width (a full `u32` here, one byte on FO3 / FNV) and `parse_fact` does not know it.
/// `None` when the record authors none of the crime sub-records.
pub fn parse_fact_crime(subs: &[SubRecord], remap: &Option<FormIdRemap>) -> Option<FactionCrime> {
    let mut crime = FactionCrime::default();
    let mut any = false;
    for sub in subs {
        let form = |sub: &SubRecord| remap_fid(SubReader::new(&sub.data).u32_or_default(), remap);
        match &sub.sub_type {
            b"DATA" if sub.data.len() >= 4 => {
                crime.flags = SubReader::new(&sub.data).u32_or_default();
                any = true;
            }
            b"JAIL" if sub.data.len() >= 4 => crime.exterior_jail_marker = form(sub),
            b"WAIT" if sub.data.len() >= 4 => crime.follower_wait_marker = form(sub),
            b"STOL" if sub.data.len() >= 4 => crime.stolen_goods_container = form(sub),
            b"PLCN" if sub.data.len() >= 4 => crime.player_inventory_container = form(sub),
            b"CRGR" if sub.data.len() >= 4 => crime.shared_crime_factions = form(sub),
            b"JOUT" if sub.data.len() >= 4 => crime.jail_outfit = form(sub),
            // CRVA: arrest u8, attack on sight u8, murder, assault, trespass, pickpocket, unknown (u16 each),
            // steal multiplier f32, escape u16, werewolf u16. The tail after `unknown` is optional.
            b"CRVA" if sub.data.len() >= CRVA_MIN_LEN => {
                let mut r = SubReader::new(&sub.data);
                let arrest = r.u8_or_default() != 0;
                let attack_on_sight = r.u8_or_default() != 0;
                let (murder, assault, trespass, pickpocket) =
                    (r.u16_or_default(), r.u16_or_default(), r.u16_or_default(), r.u16_or_default());
                let _unknown = r.u16_or_default();
                let tail = sub.data.len() >= CRVA_TAIL_LEN;
                crime.values = Some(CrimeValues {
                    arrest,
                    attack_on_sight,
                    murder,
                    assault,
                    trespass,
                    pickpocket,
                    steal_multiplier: if tail { r.f32_or_default() } else { 0.0 },
                    escape: if tail { r.u16_or_default() } else { 0 },
                    werewolf: if tail { r.u16_or_default() } else { 0 },
                });
                any = true;
            }
            _ => {}
        }
    }
    any.then_some(crime)
}
