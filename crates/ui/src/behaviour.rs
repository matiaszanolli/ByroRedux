//! M48 GUI-HOST-001…004 — host-method behaviour classes (PRD §3.2).
//!
//! | Class | Name | Meaning |
//! |---|---|---|
//! | H1 | served | the engine acts on canonical state or answers from it |
//! | H2 | placeholder | a deliberate fixed answer with a cited reason |
//! | H3 | unserved | in the catalog, no handler: the movie receives `Null` |
//! | H4 | unknown | not in the profile's catalog |
//!
//! Two views of the class, kept apart so neither claims more than is true:
//!
//! - **The catalog's** ([`HostBehaviour`]): Phase 0's per-method review
//!   (`docs/data/game-ui/host-classes-{skyrim,fallout4}.tsv`, compiled in), the class each method
//!   is *meant* to have, H1–H3, with the H2 reason and the canonical API an H1 method acts
//!   through. It is a target: most H1 rows name an API that does not exist yet ("none yet").
//! - **A call's** ([`crate::ScaleformHostCall::class`]): what happened to it. H1 only when the
//!   engine registered a handler for the method; H2 only when the method's placeholder is
//!   realized here ([`Placeholder`]); otherwise H3 for a cataloged method, whatever its target,
//!   and H4 for one the catalog lacks.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::ScaleformProfile;

const SKYRIM_CLASSES: &str = include_str!("../../../docs/data/game-ui/host-classes-skyrim.tsv");
const FALLOUT4_CLASSES: &str = include_str!("../../../docs/data/game-ui/host-classes-fallout4.tsv");

/// A behaviour class (PRD §3.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HostClass {
    /// H1.
    Served,
    /// H2.
    Placeholder,
    /// H3.
    Unserved,
    /// H4.
    Unknown,
}

impl HostClass {
    pub const ALL: [HostClass; 4] = [Self::Served, Self::Placeholder, Self::Unserved, Self::Unknown];

    /// `H1` … `H4`.
    pub fn code(self) -> &'static str {
        match self {
            Self::Served => "H1",
            Self::Placeholder => "H2",
            Self::Unserved => "H3",
            Self::Unknown => "H4",
        }
    }

    /// The PRD's report word.
    pub fn label(self) -> &'static str {
        match self {
            Self::Served => "served",
            Self::Placeholder => "placeholder",
            Self::Unserved => "unserved",
            Self::Unknown => "unknown",
        }
    }

    fn parse(code: &str) -> Option<Self> {
        Some(match code {
            "H1" => Self::Served,
            "H2" => Self::Placeholder,
            "H3" => Self::Unserved,
            _ => return None,
        })
    }
}

/// A catalog method's Phase 0 classification: its target class, the H2 reason, and the
/// canonical API an H1 method acts through (`none yet` while it does not exist).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostBehaviour {
    pub target: HostClass,
    pub reason: &'static str,
    pub canonical_api: &'static str,
}

/// How a realized H2 placeholder answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placeholder {
    /// A command the engine deliberately does nothing for: its reason says nothing needs doing
    /// (a console-only feature, a hosted service, state the movie keeps itself).
    Ignore,
    /// A request answered with one fixed boolean, read off the callback that receives it.
    Answer(bool),
}

/// The Skyrim H2 placeholders this build realizes. Every entry is an H2 row of
/// `host-classes-skyrim.tsv` whose reason fixes the behaviour; the other H2 rows stay H3 at run
/// time until their answer is established (`CanFadeItemInfo`, `GetMouseButtonForSetDestination`:
/// "answer value not established") or something produces it (`PopulateCharacterList` and its
/// siblings "answered with the single current character", `OnDisabledLoadPress`'s refusal
/// message, `ShowShoutFail`'s notification, `SetVersionText`).
///
/// The three answers are the argument of the callback the request names in `quest_journal.swf`
/// (the only movie calling them): `populateQuitList(abOnPC)` (a PC build: the PC quit list),
/// `SetShouldShowKinectTunerOption(abFlag)` (no Kinect), `SetShowMod(show)` (`true` inserts the
/// `$CREATIONS` entry, a hosted service, GUI-NG-003).
const SKYRIM_PLACEHOLDERS: &[(&str, Placeholder)] = &[
    // Platform features with no PC equivalent.
    ("ActivateJoyConStrapInstruction", Placeholder::Ignore),
    ("ChangeUser", Placeholder::Ignore),
    ("FirstTimeOunceDataTransferCheck", Placeholder::Ignore),
    ("ORBISDeleteSave", Placeholder::Ignore),
    ("OnOunceDataTransfer", Placeholder::Ignore),
    ("OnPS5DataTransfer", Placeholder::Ignore),
    ("OpenKinectTuner", Placeholder::Ignore),
    ("ResetControlsToDefaults_Ounce", Placeholder::Ignore),
    ("RequestIsOnPC", Placeholder::Answer(true)),
    ("ShouldShowKinectTunerOption", Placeholder::Answer(false)),
    // Anti-scope: hosted services.
    ("CreationClub", Placeholder::Ignore),
    ("DownloadAll", Placeholder::Ignore),
    ("MOD", Placeholder::Ignore),
    ("ModManager", Placeholder::Ignore),
    ("OpenMarketplace", Placeholder::Ignore),
    ("PopulateCreationClubTopics", Placeholder::Ignore),
    ("RequestCreationClubText", Placeholder::Ignore),
    ("ShouldShowMod", Placeholder::Answer(false)),
    ("Sky10DLCPressed", Placeholder::Ignore),
    // Nothing to do: the load order is fixed, the save list is enumerated synchronously, save
    // slots carry no thumbnail, the movie keeps its own tab and list positions, the engine
    // decides save availability, and a trace line has no canonical state.
    ("DoLoadDLCPlugins", Placeholder::Ignore),
    ("ForceStopSaveListLoading", Placeholder::Ignore),
    ("PrepSaveGameScreenshot", Placeholder::Ignore),
    ("RememberCurrentTabIndex", Placeholder::Ignore),
    ("SaveIndices", Placeholder::Ignore),
    ("SetSaveDisabled", Placeholder::Ignore),
    ("myLog", Placeholder::Ignore),
];

type Table = HashMap<&'static str, HostBehaviour>;

fn parse(tsv: &'static str) -> Table {
    let mut lines = tsv.lines();
    let header: Vec<&str> = lines.next().unwrap_or_default().split('\t').collect();
    let column = |name: &str| header.iter().position(|h| *h == name).unwrap_or_else(|| panic!("host-classes TSV lacks {name}"));
    let (method, class, reason, api) = (column("method"), column("class"), column("reason"), column("canonical_api"));
    let mut table = HashMap::new();
    for line in lines.filter(|line| !line.trim().is_empty()) {
        let fields: Vec<&'static str> = line.split('\t').collect();
        let field = |index: usize| fields.get(index).copied().unwrap_or("");
        let target = HostClass::parse(field(class)).unwrap_or_else(|| panic!("host-classes row {} has class {:?}", field(method), field(class)));
        table.insert(field(method), HostBehaviour { target, reason: field(reason), canonical_api: field(api) });
    }
    table
}

fn table(profile: ScaleformProfile) -> &'static Table {
    static SKYRIM: OnceLock<Table> = OnceLock::new();
    static FALLOUT4: OnceLock<Table> = OnceLock::new();
    match profile {
        ScaleformProfile::SkyrimAvm1 => SKYRIM.get_or_init(|| parse(SKYRIM_CLASSES)),
        ScaleformProfile::Fallout4Avm2 => FALLOUT4.get_or_init(|| parse(FALLOUT4_CLASSES)),
    }
}

/// The Phase 0 classification of `method` on `profile`, when it is a catalog method.
pub(crate) fn behaviour(profile: ScaleformProfile, method: &str) -> Option<&'static HostBehaviour> {
    table(profile).get(method)
}

/// The realized placeholder of `method` on `profile`, if any.
pub(crate) fn placeholder(profile: ScaleformProfile, method: &str) -> Option<Placeholder> {
    match profile {
        ScaleformProfile::SkyrimAvm1 => SKYRIM_PLACEHOLDERS.iter().find(|(name, _)| *name == method).map(|(_, p)| *p),
        ScaleformProfile::Fallout4Avm2 => None,
    }
}

/// One menu's calls by class (GUI-OPS-002): the count per class and the distinct H3 / H4 method
/// names, bounded by [`crate::MAX_DISTINCT_HOST_METHOD_NAMES`] like the bridge's own sets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostCallReport {
    pub counts: [u64; 4],
    pub unserved: std::collections::BTreeSet<String>,
    pub unknown: std::collections::BTreeSet<String>,
    /// A name set reached its bound and dropped later names.
    pub capped: bool,
}

impl HostCallReport {
    /// Count `call`; `true` when it is the first H3 / H4 call of its method (the drain's
    /// one-diagnostic-per-(menu, method) moment, GUI-HOST-004).
    pub fn record(&mut self, call: &crate::ScaleformHostCall) -> bool {
        let index = HostClass::ALL.iter().position(|class| *class == call.class).unwrap_or(0);
        self.counts[index] += 1;
        let set = match call.class {
            HostClass::Unserved => &mut self.unserved,
            HostClass::Unknown => &mut self.unknown,
            HostClass::Served | HostClass::Placeholder => return false,
        };
        if set.contains(&call.method) {
            return false;
        }
        if set.len() >= crate::MAX_DISTINCT_HOST_METHOD_NAMES {
            self.capped = true;
            return false;
        }
        set.insert(call.method.clone());
        true
    }

    pub fn count(&self, class: HostClass) -> u64 {
        self.counts[HostClass::ALL.iter().position(|c| *c == class).unwrap_or(0)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ScaleformHostCatalog;

    /// GUI-HOST-001 — every catalog entry of both profiles carries a Phase 0 class, and the TSVs
    /// name no method the catalog lacks.
    #[test]
    fn every_catalog_entry_has_exactly_one_phase0_class() {
        for profile in [ScaleformProfile::SkyrimAvm1, ScaleformProfile::Fallout4Avm2] {
            let catalog = ScaleformHostCatalog::for_profile(profile);
            for method in catalog.methods() {
                assert!(behaviour(profile, method.name).is_some(), "{profile:?} {} has no class", method.name);
            }
            assert_eq!(table(profile).len(), catalog.len(), "{profile:?}: the TSV names a method the catalog lacks");
        }
        let skyrim = table(ScaleformProfile::SkyrimAvm1);
        let count = |class| skyrim.values().filter(|b| b.target == class).count();
        assert_eq!((count(HostClass::Served), count(HostClass::Placeholder), count(HostClass::Unserved)), (77, 34, 31));
    }

    /// GUI-HOST-003 — every H2 row cites its reason, and online-service methods are H2 with the
    /// hosted-services reason; every realized placeholder is an H2 row, a command when it
    /// ignores and a request when it answers.
    #[test]
    fn placeholders_are_cited_h2_rows_of_the_right_kind() {
        for profile in [ScaleformProfile::SkyrimAvm1, ScaleformProfile::Fallout4Avm2] {
            for (method, b) in table(profile) {
                if b.target == HostClass::Placeholder {
                    assert!(!b.reason.trim().is_empty(), "{profile:?} H2 {method} names no reason");
                }
            }
        }
        let catalog = ScaleformHostCatalog::for_profile(ScaleformProfile::SkyrimAvm1);
        for (method, placeholder) in SKYRIM_PLACEHOLDERS {
            let b = behaviour(ScaleformProfile::SkyrimAvm1, method).unwrap();
            assert_eq!(b.target, HostClass::Placeholder, "{method}");
            let kind = catalog.find(method).unwrap().kind;
            let want = match placeholder {
                Placeholder::Ignore => crate::ScaleformHostMethodKind::Command,
                Placeholder::Answer(_) => crate::ScaleformHostMethodKind::Request,
            };
            assert_eq!(kind, want, "{method}");
        }
        for method in ["CreationClub", "DownloadAll", "OpenMarketplace", "ShouldShowMod"] {
            assert_eq!(behaviour(ScaleformProfile::SkyrimAvm1, method).unwrap().reason, "anti-scope: hosted services");
        }
    }
}
