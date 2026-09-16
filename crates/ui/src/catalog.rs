//! Profile-specific inventories of Scaleform calls made into the game host.

use crate::ScaleformProfile;

/// How a menu expects a host method to complete.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScaleformHostMethodKind {
    /// The menu does not pass a `GameDelegate` response callback.
    Command,
    /// The menu passes a callback and expects the host to invoke `respond`.
    Request,
}

/// Confidence behind a catalog entry's [`ScaleformHostMethodKind`]
/// classification. #3773 — the FO4 catalog's 269 entries mix two different
/// provenances with no marker distinguishing them: `unanswered_methods()`
/// consumers (whoever lands a handler against this catalog) previously had
/// no way to tell a `kind` read directly from source or protocol from one a
/// name-prefix heuristic guessed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScaleformKindProvenance {
    /// `kind` reflects a measured fact: Skyrim's `kind` follows directly
    /// from the AVM1 GameDelegate protocol (every entry a direct command,
    /// see [`SKYRIM_SKYUI_METHODS`]'s own doc), and the FO4 catalog's
    /// original 138 F4CF-reconstructed entries had `kind` read from
    /// reconstructed ActionScript source, not inferred from the name.
    Measured,
    /// `kind` was inferred from a `Get*`/`Is*`/`Should*`/`Can*`/`get*`
    /// name-prefix heuristic — the 131 entries #2966's corpus sweep added
    /// to the FO4 catalog. The heuristic's boundary is demonstrably
    /// imprecise (#3773): every `Request`-classified entry happens to match
    /// the prefix set (none was ever promoted to `Request` against evidence
    /// that contradicted the rule), while at least 16 `Command`-classified
    /// names carry query-shaped verbs (`Request*`, `Check*`, `Validate*`,
    /// …) the prefix set doesn't cover at all — so a `Command` here is a
    /// weaker claim than a `Measured` one.
    HeuristicNamePrefix,
}

/// One method in a profile's known host surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScaleformHostMethod {
    pub name: &'static str,
    pub kind: ScaleformHostMethodKind,
    pub provenance: ScaleformKindProvenance,
}

/// Native object installed by a profile on the root ActionScript object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScaleformHostObject {
    /// Dynamic property containing the native function table.
    pub property: &'static str,
    /// Root callback invoked after the function table is populated.
    pub on_create: &'static str,
    /// Root callback invoked before the native object is released.
    pub on_destroy: &'static str,
}

impl ScaleformHostMethod {
    const fn command(name: &'static str) -> Self {
        Self {
            name,
            kind: ScaleformHostMethodKind::Command,
            provenance: ScaleformKindProvenance::Measured,
        }
    }

    const fn request(name: &'static str) -> Self {
        Self {
            name,
            kind: ScaleformHostMethodKind::Request,
            provenance: ScaleformKindProvenance::Measured,
        }
    }

    /// #3773 — sibling of [`Self::command`] for one of the #2966 sweep's
    /// 131 name-prefix-inferred FO4 entries. See
    /// [`ScaleformKindProvenance::HeuristicNamePrefix`].
    const fn command_heuristic(name: &'static str) -> Self {
        Self {
            name,
            kind: ScaleformHostMethodKind::Command,
            provenance: ScaleformKindProvenance::HeuristicNamePrefix,
        }
    }

    /// #3773 — sibling of [`Self::request`] for one of the #2966 sweep's
    /// 131 name-prefix-inferred FO4 entries. See
    /// [`ScaleformKindProvenance::HeuristicNamePrefix`].
    const fn request_heuristic(name: &'static str) -> Self {
        Self {
            name,
            kind: ScaleformHostMethodKind::Request,
            provenance: ScaleformKindProvenance::HeuristicNamePrefix,
        }
    }
}

/// Read-only host-method catalog selected by the active runtime profile.
#[derive(Clone, Copy, Debug)]
pub struct ScaleformHostCatalog {
    profile: ScaleformProfile,
}

impl ScaleformHostCatalog {
    pub const fn for_profile(profile: ScaleformProfile) -> Self {
        Self { profile }
    }

    pub const fn profile(self) -> ScaleformProfile {
        self.profile
    }

    pub fn methods(self) -> &'static [ScaleformHostMethod] {
        match self.profile {
            ScaleformProfile::SkyrimAvm1 => SKYRIM_SKYUI_METHODS,
            ScaleformProfile::Fallout4Avm2 => FALLOUT4_BGS_CODE_OBJECT_METHODS,
        }
    }

    pub const fn host_object(self) -> Option<ScaleformHostObject> {
        match self.profile {
            ScaleformProfile::SkyrimAvm1 => None,
            ScaleformProfile::Fallout4Avm2 => Some(ScaleformHostObject {
                property: "BGSCodeObj",
                on_create: "onCodeObjCreate",
                on_destroy: "onCodeObjDestruction",
            }),
        }
    }

    pub fn find(self, name: &str) -> Option<&'static ScaleformHostMethod> {
        self.methods()
            .binary_search_by(|method| method.name.cmp(name))
            .ok()
            .map(|index| &self.methods()[index])
    }

    pub fn contains(self, name: &str) -> bool {
        self.find(name).is_some()
    }

    pub fn is_empty(self) -> bool {
        self.methods().is_empty()
    }

    pub fn len(self) -> usize {
        self.methods().len()
    }
}

// Literal GameDelegate.call sites in SkyUI's Skyrim interface sources, pinned
// to tree 835428728e2305865e220fdfc99d791434955eb1. Entries with a fourth
// callback argument are requests; the remainder are commands.
//
// Source: https://github.com/schlangster/skyui/tree/master/src
//
// #3103 — now *measured* against the shipped corpus, not only read off that
// snapshot. `avm1_host::tests::installed_skyrim_host_calls_are_all_cataloged`
// walks every `GameDelegate.call` site in all 53 movies of `Skyrim -
// Interface.bsa`: 141 distinct host methods across 35 movies, of which **68
// are missing from this table** and exactly 1 entry here (`SliderClose`) is
// never called by a vanilla menu. A further 72 call sites pass a runtime name
// (`this.callbackName` and friends) that no static walk can resolve.
//
// So this table *was* close to a subset of what the game actually calls, which
// is the expected shape for a SkyUI-sourced list — SkyUI replaces these menus.
//
// #3103, regeneration half — those 68 are now merged in, bringing this array
// to 142 (the 141 the sweep found plus `SliderClose`). The blocker recorded
// here was that SkyUI's "a fourth argument means request" test cannot classify
// them, because every vanilla call site passes exactly two arguments. That is
// still true, and it is why they are NOT `Measured`: they carry #3773's
// `HeuristicNamePrefix` provenance and are classified by the same name-prefix
// rule #2966 applied to Fallout 4 (`Get*` / `Is*` / `Should*` / `Can*` /
// `get*`, camelCase-boundary matched so `Cancel` does not read as `Can*`).
// Only 2 of the 68 match (`GetMouseButtonForSetDestination`, `ShouldShowMod`);
// the other 66 default to `Command`.
//
// Landing a guess here is sound for the same reason it was for Fallout 4, and
// the reason is worth restating because it is what makes the difference
// between a guess and a lie: `kind` selects a *diagnostic bucket only*.
// `host.rs::record_call` queues every call onto `state.calls` whatever its
// dispatch, and computes the return value from the configured response
// regardless of `kind`. A misclassified entry therefore moves a name between
// `unanswered_methods()` and a plain `Queued`; it cannot drop a call, stall a
// menu or change a returned value. What it *does* buy is that these 68 names
// stop arriving as `unknown_methods()` — a bucket that should mean "this build
// has never heard of this method", not "we measured it and can't type it".
//
// A fourth-argument call site appearing in some future corpus (a SkyUI install,
// a DLC archive) is evidence that promotes an entry to `Measured`; the
// provenance split is pinned exactly so that has to be a deliberate edit.
static SKYRIM_SKYUI_METHODS: &[ScaleformHostMethod] = &[
    ScaleformHostMethod::command_heuristic("ActivateJoyConStrapInstruction"),
    ScaleformHostMethod::command("AuxButtonPress"),
    ScaleformHostMethod::command_heuristic("CONTINUE"),
    ScaleformHostMethod::request("CalculateCharge"),
    ScaleformHostMethod::request("CanFadeItemInfo"),
    ScaleformHostMethod::command_heuristic("Cancel"),
    ScaleformHostMethod::command_heuristic("ChangeDoubleMorph"),
    ScaleformHostMethod::command_heuristic("ChangeFaceDetails"),
    ScaleformHostMethod::command_heuristic("ChangeHairColorPreset"),
    ScaleformHostMethod::command_heuristic("ChangeHeadPart"),
    ScaleformHostMethod::command_heuristic("ChangeHeadPreset"),
    ScaleformHostMethod::command_heuristic("ChangeName"),
    ScaleformHostMethod::command_heuristic("ChangePreset"),
    ScaleformHostMethod::command_heuristic("ChangeRace"),
    ScaleformHostMethod::command_heuristic("ChangeTintingMask"),
    ScaleformHostMethod::command_heuristic("ChangeUser"),
    ScaleformHostMethod::command_heuristic("ChangeWeight"),
    ScaleformHostMethod::command_heuristic("CharacterSelected"),
    ScaleformHostMethod::request("CheckForMouseEquip"),
    ScaleformHostMethod::command("ChooseItem"),
    ScaleformHostMethod::command("ClickCallback"),
    ScaleformHostMethod::command("CloseMenu"),
    ScaleformHostMethod::command("CloseTweenMenu"),
    ScaleformHostMethod::command_heuristic("ConfirmDone"),
    ScaleformHostMethod::command("CraftButtonPress"),
    ScaleformHostMethod::command("CraftSelectedItem"),
    ScaleformHostMethod::command_heuristic("CreationClub"),
    ScaleformHostMethod::command("CurrentLocationCallback"),
    ScaleformHostMethod::command("DeleteSave"),
    ScaleformHostMethod::command("DisabledItemSelect"),
    ScaleformHostMethod::command_heuristic("DoDeleteSaveUISanityCheck"),
    ScaleformHostMethod::command_heuristic("DoLoadDLCPlugins"),
    ScaleformHostMethod::command_heuristic("DownloadAll"),
    ScaleformHostMethod::command("EndItemRename"),
    ScaleformHostMethod::command_heuristic("EndPressStartState"),
    ScaleformHostMethod::command("EquipItem"),
    ScaleformHostMethod::command_heuristic("ExecuteCommand"),
    ScaleformHostMethod::command_heuristic("Exit"),
    ScaleformHostMethod::command("FadeDone"),
    ScaleformHostMethod::command_heuristic("FirstTimeOunceDataTransferCheck"),
    ScaleformHostMethod::command_heuristic("ForceStopSaveListLoading"),
    ScaleformHostMethod::request("GetButtonFromUserEvent"),
    ScaleformHostMethod::request_heuristic("GetMouseButtonForSetDestination"),
    ScaleformHostMethod::request("GetRawDealWarningString"),
    ScaleformHostMethod::command_heuristic("HELP"),
    ScaleformHostMethod::command_heuristic("HideComplete"),
    ScaleformHostMethod::command_heuristic("HighlightMenu"),
    ScaleformHostMethod::command("IsOKtoLoad"),
    ScaleformHostMethod::command("ItemCardListCallback"),
    ScaleformHostMethod::command("ItemDrop"),
    ScaleformHostMethod::command("ItemSelect"),
    ScaleformHostMethod::command("ItemTransfer"),
    ScaleformHostMethod::command("LOAD"),
    ScaleformHostMethod::command_heuristic("LoadDLC"),
    ScaleformHostMethod::command("LoadGame"),
    ScaleformHostMethod::command_heuristic("MOD"),
    ScaleformHostMethod::command("MarkerClick"),
    ScaleformHostMethod::command_heuristic("ModManager"),
    ScaleformHostMethod::command_heuristic("MoveCamera"),
    ScaleformHostMethod::command_heuristic("NEW"),
    ScaleformHostMethod::command_heuristic("OK"),
    ScaleformHostMethod::command_heuristic("ORBISDeleteSave"),
    ScaleformHostMethod::command_heuristic("OnDisabledLoadPress"),
    ScaleformHostMethod::command_heuristic("OnOunceDataTransfer"),
    ScaleformHostMethod::command_heuristic("OnPS5DataTransfer"),
    ScaleformHostMethod::command_heuristic("OpenAnimFinished"),
    ScaleformHostMethod::command_heuristic("OpenHighlightedMenu"),
    ScaleformHostMethod::command("OpenJournalCallback"),
    ScaleformHostMethod::command("OpenKinectTuner"),
    ScaleformHostMethod::command_heuristic("OpenMarketplace"),
    ScaleformHostMethod::command("OptionChange"),
    ScaleformHostMethod::command("PlaySound"),
    ScaleformHostMethod::command_heuristic("PopulateCharacterList"),
    ScaleformHostMethod::command_heuristic("PopulateCreationClubTopics"),
    ScaleformHostMethod::command("PopulateHelpTopics"),
    ScaleformHostMethod::command("PrepSaveGameScreenshot"),
    ScaleformHostMethod::command("QuantitySliderOpen"),
    ScaleformHostMethod::command_heuristic("QuickSave"),
    ScaleformHostMethod::command("QuitToDesktop"),
    ScaleformHostMethod::command("QuitToMainMenu"),
    ScaleformHostMethod::command_heuristic("RegisterHUDComponents"),
    ScaleformHostMethod::command_heuristic("RememberCurrentTabIndex"),
    ScaleformHostMethod::command("RequestAudioOptions"),
    ScaleformHostMethod::command_heuristic("RequestCreationClubText"),
    ScaleformHostMethod::command("RequestDisplayOptions"),
    ScaleformHostMethod::command("RequestGameplayOptions"),
    ScaleformHostMethod::command("RequestHelpText"),
    ScaleformHostMethod::command("RequestInputMappings"),
    ScaleformHostMethod::request("RequestIsOnPC"),
    ScaleformHostMethod::request("RequestItemCardInfo"),
    ScaleformHostMethod::command_heuristic("RequestLoadingText"),
    ScaleformHostMethod::command("RequestObjectivesData"),
    ScaleformHostMethod::request("RequestPlayerInfo"),
    ScaleformHostMethod::request("RequestQuestsData"),
    ScaleformHostMethod::command("ResetControlsToDefaults"),
    ScaleformHostMethod::command_heuristic("ResetControlsToDefaults_Ounce"),
    ScaleformHostMethod::command("SAVE"),
    ScaleformHostMethod::command("SaveControls"),
    ScaleformHostMethod::command("SaveGame"),
    ScaleformHostMethod::command("SaveIndices"),
    ScaleformHostMethod::command("SaveSettings"),
    ScaleformHostMethod::command_heuristic("SetAllowTextInput"),
    ScaleformHostMethod::command_heuristic("SetFadedIn"),
    ScaleformHostMethod::command("SetLocalMapExtents"),
    ScaleformHostMethod::command("SetSaveDisabled"),
    ScaleformHostMethod::command("SetSelectedCategory"),
    ScaleformHostMethod::command("SetSelectedItem"),
    ScaleformHostMethod::command("SetVersionText"),
    ScaleformHostMethod::request("ShouldShowKinectTunerOption"),
    ScaleformHostMethod::request_heuristic("ShouldShowMod"),
    ScaleformHostMethod::command("ShowItem3D"),
    ScaleformHostMethod::command("ShowShoutFail"),
    ScaleformHostMethod::command("ShowSoulGemList"),
    ScaleformHostMethod::command("ShowTargetOnMap"),
    ScaleformHostMethod::command("ShowTweenMenu"),
    ScaleformHostMethod::command_heuristic("ShowVirtualKeyboard"),
    ScaleformHostMethod::command_heuristic("SkipText"),
    ScaleformHostMethod::command_heuristic("Sky10DLCPressed"),
    ScaleformHostMethod::command("SliderClose"),
    ScaleformHostMethod::command_heuristic("StartCloseMenu"),
    ScaleformHostMethod::command("StartMouseRotation"),
    ScaleformHostMethod::command("StartRemapMode"),
    ScaleformHostMethod::command_heuristic("StartState"),
    ScaleformHostMethod::command("StopMouseRotation"),
    ScaleformHostMethod::command("TakeAllItems"),
    ScaleformHostMethod::command("ToggleMapCallback"),
    ScaleformHostMethod::request("ToggleQuestActiveStatus"),
    ScaleformHostMethod::command("ToggleShowMiscObjectives"),
    ScaleformHostMethod::command_heuristic("TopicClicked"),
    ScaleformHostMethod::command_heuristic("Train"),
    ScaleformHostMethod::command("UpdateItem3D"),
    ScaleformHostMethod::command_heuristic("UseCurrentCharacterFilter"),
    ScaleformHostMethod::command("ZoomItemModel"),
    ScaleformHostMethod::command_heuristic("ZoomPC"),
    ScaleformHostMethod::command_heuristic("addHealth"),
    ScaleformHostMethod::command_heuristic("addMagicka"),
    ScaleformHostMethod::command_heuristic("addStamina"),
    ScaleformHostMethod::command("buttonPress"),
    ScaleformHostMethod::command_heuristic("currentState"),
    ScaleformHostMethod::command_heuristic("fadeOutStarted"),
    ScaleformHostMethod::command_heuristic("myLog"),
    ScaleformHostMethod::request("updateStats"),
];

// #2966 — regenerated from `installed_fallout4_host_calls_are_all_forwarded`'s
// corpus sweep against `Fallout4 - Interface.ba2` (311 movies, 2026-08-19),
// not just from F4CF/Interface's reconstructed sources. That sweep measures
// BOTH directions: every `BGSCodeObj.<method>` call site actually shipped
// across the whole archive, and which catalog entries nothing in it calls.
//
// The array below is the union: the original 138 F4CF-reconstructed entries
// plus the 131 real call sites the sweep found outside them — 269 total,
// covering the corpus this repo can measure. `kind` is classified by name
// prefix (`Get*` / `Is*` / `Should*` / `Can*` / `get*`, camelCase-boundary
// matched so `Cancel`/`CancelPlayback` don't false-positive on `Can*`) as a
// first pass per #2966's suggested fix — BGSCodeObj has no GameDelegate-style
// callback protocol, so `Request` here means "the sweep's own inventory + a
// naming convention marks this a query", not "this movie passed a response
// callback"; getting it wrong only misroutes a diagnostic bucket
// (`unanswered_methods()`), never the actual `ExternalInterface` return value
// (see `host.rs::record_call` — `dispatch` is bookkeeping, `return_value` is
// computed from the configured response regardless of `kind`).
//
// 45 of the original 138 are unreferenced by this one archive. Kept, not
// deleted: this sweep only covers the base-game interface archive, and FO4
// ships additional Scaleform-driving DLC (Far Harbor, Nuka-World, Vault-Tec
// Workshop, Automatron, Contraptions/Wasteland/Vault-Tec DLC) and
// Creation-Club content in their own archives this repo doesn't have on disk
// to sweep — an unreferenced-here entry is not evidence it's unused by any
// shipped Fallout 4 content.
//
// Source: https://github.com/F4CF/Interface/tree/master/Data/Interface/Source/Bethesda
static FALLOUT4_BGS_CODE_OBJECT_METHODS: &[ScaleformHostMethod] = &[
    ScaleformHostMethod::command_heuristic("Accept"),
    ScaleformHostMethod::command("ActivateScrollSound"),
    ScaleformHostMethod::command_heuristic("AreModsLoaded"),
    ScaleformHostMethod::command("BackLevel"),
    ScaleformHostMethod::command_heuristic("CClubBlockedByBnet"),
    ScaleformHostMethod::command_heuristic("CClubBlockedByPermissions"),
    ScaleformHostMethod::request("CanRepairSelectedItem"),
    ScaleformHostMethod::command_heuristic("Cancel"),
    ScaleformHostMethod::command_heuristic("CancelPlayback"),
    ScaleformHostMethod::command("CenterMarkerRequest"),
    ScaleformHostMethod::command_heuristic("ChangeBeard"),
    ScaleformHostMethod::command_heuristic("ChangeCharacterPreset"),
    ScaleformHostMethod::command_heuristic("ChangeColor"),
    ScaleformHostMethod::command_heuristic("ChangeHairColor"),
    ScaleformHostMethod::command_heuristic("ChangeHairStyle"),
    ScaleformHostMethod::command_heuristic("ChangePreset"),
    ScaleformHostMethod::command_heuristic("ChangePresetIntensity"),
    ScaleformHostMethod::command_heuristic("ChangeSex"),
    ScaleformHostMethod::command("CheckHardcoreModeFastTravel"),
    ScaleformHostMethod::command("CheckRequirements"),
    ScaleformHostMethod::command_heuristic("ClearBoneRegionTint"),
    ScaleformHostMethod::command_heuristic("ClearDetails"),
    ScaleformHostMethod::command_heuristic("ClearPickData"),
    ScaleformHostMethod::command("ClearPlayerMarker"),
    ScaleformHostMethod::command_heuristic("ClearTemporaryDetail"),
    ScaleformHostMethod::command("CloseMenu"),
    ScaleformHostMethod::command_heuristic("ConfirmAndCloseMenu"),
    ScaleformHostMethod::command("ConfirmBuild"),
    ScaleformHostMethod::command_heuristic("ContinueGame"),
    ScaleformHostMethod::command_heuristic("CreateSavePoint"),
    ScaleformHostMethod::command_heuristic("CreateUndoPoint"),
    ScaleformHostMethod::command_heuristic("CycleBodyPart"),
    ScaleformHostMethod::command_heuristic("CycleTarget"),
    ScaleformHostMethod::command_heuristic("DeleteDLC"),
    ScaleformHostMethod::command_heuristic("DeleteSave"),
    ScaleformHostMethod::command_heuristic("DoQuicksave"),
    ScaleformHostMethod::command_heuristic("DownloadAll"),
    ScaleformHostMethod::command_heuristic("DownloadCreation"),
    ScaleformHostMethod::command_heuristic("EndBodyEdit"),
    ScaleformHostMethod::command("EndRotate3DItem"),
    ScaleformHostMethod::command_heuristic("EnterCreationClub"),
    ScaleformHostMethod::command_heuristic("EnterCreditsScreen"),
    ScaleformHostMethod::command_heuristic("EnterCreditsScreenFromExpandedMenu"),
    ScaleformHostMethod::command_heuristic("EnterDetailsScreen"),
    ScaleformHostMethod::command("ExamineItem"),
    ScaleformHostMethod::command_heuristic("ExecuteCritical"),
    ScaleformHostMethod::command("FastTravel"),
    ScaleformHostMethod::command("FillModPartArray"),
    ScaleformHostMethod::command_heuristic("FinishLoadGame"),
    ScaleformHostMethod::command_heuristic("FinishSaveGame"),
    ScaleformHostMethod::request("GetButtonFromUserEvent"),
    ScaleformHostMethod::request_heuristic("GetDetailColor"),
    ScaleformHostMethod::request_heuristic("GetDetailColorCount"),
    ScaleformHostMethod::request_heuristic("GetDetailIntensity"),
    ScaleformHostMethod::request("GetDisplayRate"),
    ScaleformHostMethod::request_heuristic("GetExtraGroupName"),
    ScaleformHostMethod::request_heuristic("GetFeatureData"),
    ScaleformHostMethod::request("GetHackingBoardCharHeight"),
    ScaleformHostMethod::request("GetHackingBoardCharWidth"),
    ScaleformHostMethod::request_heuristic("GetHasDetailsApplied"),
    ScaleformHostMethod::request_heuristic("GetHasInstalledContent"),
    ScaleformHostMethod::request_heuristic("GetHasSavedGames"),
    ScaleformHostMethod::request_heuristic("GetLastCharacterPreset"),
    ScaleformHostMethod::request("GetNumGuesses"),
    ScaleformHostMethod::request("GetPerkInfoByRank"),
    ScaleformHostMethod::request_heuristic("GetShowBethesdaNetOption"),
    ScaleformHostMethod::request_heuristic("GetShowCreationClubOption"),
    ScaleformHostMethod::request("GetStartingListPosition"),
    ScaleformHostMethod::request("GetXPInfo"),
    ScaleformHostMethod::command("HideMenu"),
    ScaleformHostMethod::command_heuristic("HighlightBoneRegion"),
    ScaleformHostMethod::command("HolotapeActivate"),
    ScaleformHostMethod::command_heuristic("InitCreationClub"),
    ScaleformHostMethod::command_heuristic("InitLoginObject"),
    ScaleformHostMethod::command_heuristic("InitialPopulateLoadList"),
    ScaleformHostMethod::request_heuristic("IsDLCReady"),
    ScaleformHostMethod::request_heuristic("IsMainMenuReady"),
    ScaleformHostMethod::request("IsSelectedItemEquipped"),
    ScaleformHostMethod::command("ItemDrop"),
    ScaleformHostMethod::command("ItemSelect"),
    ScaleformHostMethod::command_heuristic("Land"),
    ScaleformHostMethod::command_heuristic("ModsBlockedByBnet"),
    ScaleformHostMethod::command_heuristic("NotifyForWittyBanter"),
    ScaleformHostMethod::command("OnAcceptPress"),
    ScaleformHostMethod::command("OnAlternateButton"),
    ScaleformHostMethod::command_heuristic("OnAnimateOutComplete"),
    ScaleformHostMethod::command("OnBuildFailed"),
    ScaleformHostMethod::command("OnMenuItemSelect"),
    ScaleformHostMethod::command("OnMobileSettingsLoaded"),
    ScaleformHostMethod::command_heuristic("OnPS5DataTransfer"),
    ScaleformHostMethod::command("OnScrollingStarted"),
    ScaleformHostMethod::command("OnScrollingStopped"),
    ScaleformHostMethod::command("OnSpeechChallengeAnimComplete"),
    ScaleformHostMethod::command_heuristic("PlayCancelSound"),
    ScaleformHostMethod::command("PlayFocusSound"),
    ScaleformHostMethod::command_heuristic("PlayOKSound"),
    ScaleformHostMethod::command("PlayPerkSound"),
    ScaleformHostMethod::command_heuristic("PlayPopupSound"),
    ScaleformHostMethod::command("PlaySmallTransition"),
    ScaleformHostMethod::command("PlaySound"),
    ScaleformHostMethod::command_heuristic("PlayTabLeftSound"),
    ScaleformHostMethod::command_heuristic("PlayTabRightSound"),
    ScaleformHostMethod::command("PlayZoomSound"),
    ScaleformHostMethod::command_heuristic("PopulateCharacterList"),
    ScaleformHostMethod::command_heuristic("PopulateDLCList"),
    ScaleformHostMethod::command_heuristic("PopulateHelpTopics"),
    ScaleformHostMethod::command_heuristic("PopulateInstalledContentTopics"),
    ScaleformHostMethod::command_heuristic("PopulateLoadList"),
    ScaleformHostMethod::command("PopulatePipboyInfoObj"),
    ScaleformHostMethod::command_heuristic("PopulateSaveList"),
    ScaleformHostMethod::command_heuristic("PurchaseDLC"),
    ScaleformHostMethod::command_heuristic("PurchaseMod"),
    ScaleformHostMethod::command_heuristic("QueueAction"),
    ScaleformHostMethod::command("RefreshMapMarkers"),
    ScaleformHostMethod::command("RegisterComponents"),
    ScaleformHostMethod::command("RegisterMap"),
    ScaleformHostMethod::command("RegisterMovie"),
    ScaleformHostMethod::command("RegisterPerkGridComponents"),
    ScaleformHostMethod::command_heuristic("RegisterSaveLoadPanel"),
    ScaleformHostMethod::command("RegisterTerminalElements"),
    ScaleformHostMethod::command("RemoveHighlight"),
    ScaleformHostMethod::command("RepairSelectedItem"),
    ScaleformHostMethod::command_heuristic("RequestAudioOptions"),
    ScaleformHostMethod::command_heuristic("RequestDisplayOptions"),
    ScaleformHostMethod::command_heuristic("RequestGameplayOptions"),
    ScaleformHostMethod::command_heuristic("RequestHelpText"),
    ScaleformHostMethod::command_heuristic("RequestHelpTitle"),
    ScaleformHostMethod::command_heuristic("RequestInputMappings"),
    ScaleformHostMethod::command_heuristic("RequestInstalledContentText"),
    ScaleformHostMethod::command_heuristic("RequestInstalledContentTitle"),
    ScaleformHostMethod::command_heuristic("RequestRefreshInstallProgress"),
    ScaleformHostMethod::command_heuristic("ResetControlsToDefaults"),
    ScaleformHostMethod::command_heuristic("ReturnFromDLC"),
    ScaleformHostMethod::command("RevertChanges"),
    ScaleformHostMethod::command_heuristic("SaveSettings"),
    ScaleformHostMethod::command("ScrapItem"),
    ScaleformHostMethod::command("SelectHackingWord"),
    ScaleformHostMethod::command("SelectItem"),
    ScaleformHostMethod::command("SelectPerk"),
    ScaleformHostMethod::command("SendTutorialEvent"),
    ScaleformHostMethod::command_heuristic("SetBackgroundVisible"),
    ScaleformHostMethod::command_heuristic("SetBumpersRepeat"),
    ScaleformHostMethod::command_heuristic("SetCurrentCharacter"),
    ScaleformHostMethod::command_heuristic("SetDetailColor"),
    ScaleformHostMethod::command_heuristic("SetDetailIntensity"),
    ScaleformHostMethod::command_heuristic("SetHairHighlight"),
    ScaleformHostMethod::command("SetItemSelectValuesForComponents"),
    ScaleformHostMethod::command("SetName"),
    ScaleformHostMethod::command("SetPlayerMarker"),
    ScaleformHostMethod::command("SetQuestActive"),
    ScaleformHostMethod::command("SetQuickkey"),
    ScaleformHostMethod::request("ShouldShowTagForSearchButton"),
    ScaleformHostMethod::command_heuristic("ShowChangeUser"),
    ScaleformHostMethod::command_heuristic("ShowContinueSecondPanel"),
    ScaleformHostMethod::command_heuristic("ShowCreditMenu"),
    ScaleformHostMethod::command("ShowItem"),
    ScaleformHostMethod::command("ShowPerksMenu"),
    ScaleformHostMethod::command_heuristic("ShowPlatformHelp"),
    ScaleformHostMethod::command("ShowQuestOnMap"),
    ScaleformHostMethod::command("ShowWorkshopOnMap"),
    ScaleformHostMethod::command("SortItemList"),
    ScaleformHostMethod::command_heuristic("StartBodyEdit"),
    ScaleformHostMethod::command("StartBuildConfirm"),
    ScaleformHostMethod::command("StartItemSelection"),
    ScaleformHostMethod::command_heuristic("StartNewGame"),
    ScaleformHostMethod::command_heuristic("StartPlayback"),
    ScaleformHostMethod::command_heuristic("StartRemapMode"),
    ScaleformHostMethod::command("StartRotate3DItem"),
    ScaleformHostMethod::command_heuristic("StartWaiting"),
    ScaleformHostMethod::command("StopPerkSound"),
    ScaleformHostMethod::command("SwitchBaseItem"),
    ScaleformHostMethod::command("SwitchMod"),
    ScaleformHostMethod::command("ToggleComponentFavorite"),
    ScaleformHostMethod::command("ToggleFavoriteMod"),
    ScaleformHostMethod::command("ToggleItemEquipped"),
    ScaleformHostMethod::command("ToggleRadioStationActiveStatus"),
    ScaleformHostMethod::command_heuristic("UndoLastAction"),
    ScaleformHostMethod::command("UnregisterMap"),
    ScaleformHostMethod::command_heuristic("UpdateDLC"),
    ScaleformHostMethod::command("UpdateRequirements"),
    ScaleformHostMethod::command_heuristic("UpsellPressed"),
    ScaleformHostMethod::command("UseRadaway"),
    ScaleformHostMethod::command("UseStimpak"),
    ScaleformHostMethod::command("ValidateHackingWord"),
    ScaleformHostMethod::command_heuristic("WeightPointChange"),
    ScaleformHostMethod::command("ZoomIn"),
    ScaleformHostMethod::command("ZoomOut"),
    ScaleformHostMethod::command_heuristic("attemptCloseManager"),
    ScaleformHostMethod::command("closeHolotape"),
    ScaleformHostMethod::command("closeMenu"),
    ScaleformHostMethod::command_heuristic("confirmCloseManager"),
    ScaleformHostMethod::command("confirmInvest"),
    ScaleformHostMethod::command_heuristic("confirmResetPoints"),
    ScaleformHostMethod::command("executeCommand"),
    ScaleformHostMethod::command("exitMenu"),
    ScaleformHostMethod::request("getButtonFromUserEvent"),
    ScaleformHostMethod::request("getHighscore"),
    ScaleformHostMethod::request("getItemValue"),
    ScaleformHostMethod::request_heuristic("getSaveData"),
    ScaleformHostMethod::request("getScrollSpeed"),
    ScaleformHostMethod::request("getSelectedItemEquippable"),
    ScaleformHostMethod::request("getSelectedItemEquipped"),
    ScaleformHostMethod::request_heuristic("getTextReplaceID"),
    ScaleformHostMethod::request_heuristic("getTextReplaceValue"),
    ScaleformHostMethod::command_heuristic("initMenu"),
    ScaleformHostMethod::command("inspectItem"),
    ScaleformHostMethod::command_heuristic("modValue"),
    ScaleformHostMethod::command_heuristic("notifyScripts"),
    ScaleformHostMethod::command("onAcceptPress"),
    ScaleformHostMethod::command("onBackButtonHandled"),
    ScaleformHostMethod::command("onButtonPress"),
    ScaleformHostMethod::command("onButtonRelease"),
    ScaleformHostMethod::command_heuristic("onCancel"),
    ScaleformHostMethod::command("onCancelPress"),
    ScaleformHostMethod::command("onComponentViewToggle"),
    ScaleformHostMethod::command_heuristic("onContinuePress"),
    ScaleformHostMethod::command_heuristic("onDisabledLoadPress"),
    ScaleformHostMethod::command("onFadeDone"),
    ScaleformHostMethod::command("onGPSModeButtonClicked"),
    ScaleformHostMethod::command("onGridAddedToStage"),
    ScaleformHostMethod::command_heuristic("onHUDColorUpdate"),
    ScaleformHostMethod::command("onHideComplete"),
    ScaleformHostMethod::command("onIntroAnimComplete"),
    ScaleformHostMethod::command("onInvItemSelection"),
    ScaleformHostMethod::command_heuristic("onLocationConfirm"),
    ScaleformHostMethod::command("onManualModeButtonClicked"),
    ScaleformHostMethod::command("onMenuLoadComplete"),
    ScaleformHostMethod::command("onModalOpen"),
    ScaleformHostMethod::command("onNewPage"),
    ScaleformHostMethod::command_heuristic("onNewPress"),
    ScaleformHostMethod::command("onNewTab"),
    ScaleformHostMethod::command("onPerksTabClose"),
    ScaleformHostMethod::command("onPerksTabOpen"),
    ScaleformHostMethod::command_heuristic("onPipboyColorUpdate"),
    ScaleformHostMethod::command("onQuestSelection"),
    ScaleformHostMethod::command_heuristic("onQuitPress"),
    ScaleformHostMethod::command_heuristic("onQuitToDesktopPress"),
    ScaleformHostMethod::command("onScanButtonClicked"),
    ScaleformHostMethod::command_heuristic("onSettingsValueChange"),
    ScaleformHostMethod::command("onShowHotKeys"),
    ScaleformHostMethod::command("onSwitchBetweenWorldLocalMap"),
    ScaleformHostMethod::command("pauseRegisteredSound"),
    ScaleformHostMethod::command("playActionAnim"),
    ScaleformHostMethod::command_heuristic("playLeftAnim"),
    ScaleformHostMethod::command("playRegisteredSound"),
    ScaleformHostMethod::command_heuristic("playRightAnim"),
    ScaleformHostMethod::command("playSound"),
    ScaleformHostMethod::command_heuristic("populateCaravanList"),
    ScaleformHostMethod::command("registerObjects"),
    ScaleformHostMethod::command("registerSound"),
    ScaleformHostMethod::command("requestCredits"),
    ScaleformHostMethod::command_heuristic("requestLoadingText"),
    ScaleformHostMethod::command("sendXButton"),
    ScaleformHostMethod::command("sendYButton"),
    ScaleformHostMethod::command("setHighscore"),
    ScaleformHostMethod::command_heuristic("setSaveData"),
    ScaleformHostMethod::command("show3D"),
    ScaleformHostMethod::command("sortItems"),
    ScaleformHostMethod::command_heuristic("startEditText"),
    ScaleformHostMethod::command("stopRegisteredSound"),
    ScaleformHostMethod::command("takeAllItems"),
    ScaleformHostMethod::command("toggleMovementToDirectional"),
    ScaleformHostMethod::command("toggleSelectedItemEquipped"),
    ScaleformHostMethod::command("transferItem"),
    ScaleformHostMethod::command_heuristic("tryCloseMenu"),
    ScaleformHostMethod::command("updateItem3D"),
    ScaleformHostMethod::command("updateItemPickpocketInfo"),
    ScaleformHostMethod::command("updateSortButtonLabel"),
    ScaleformHostMethod::command("useQuickkey"),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// `find`/`contains` binary-search `methods()`, so every
    /// profile's array must stay in strict ascending `str::cmp` order — a
    /// manually inserted entry that lands even one position out of place
    /// silently breaks lookups for names that happen to binary-search past
    /// it, with no compiler or runtime error to catch it. #2966 regenerated
    /// the 269-entry Fallout 4 array by hand; this pins the invariant that
    /// makes doing so safe.
    fn assert_sorted_and_unique(methods: &[ScaleformHostMethod]) {
        for window in methods.windows(2) {
            assert!(
                window[0].name < window[1].name,
                "catalog out of order (or duplicated) around {:?} / {:?} — \
                 find()/contains() binary-search this array and require \
                 strict ascending order",
                window[0].name,
                window[1].name,
            );
        }
    }

    #[test]
    fn skyrim_catalog_is_sorted_and_unique() {
        assert_sorted_and_unique(SKYRIM_SKYUI_METHODS);
    }

    #[test]
    fn fallout4_catalog_is_sorted_and_unique() {
        assert_sorted_and_unique(FALLOUT4_BGS_CODE_OBJECT_METHODS);
    }

    /// #3773 — pins the exact provenance split #2966's commit recorded:
    /// 138 `Measured` (F4CF-reconstructed) + 131 `HeuristicNamePrefix`
    /// (the corpus-sweep addition), derived here by re-deriving the exact
    /// same partition the fix itself was built from (`git show 0a87ca54`'s
    /// before/after name sets) — a regression back toward "no marker at
    /// all", or a future addition that doesn't call the right constructor,
    /// changes this count.
    #[test]
    fn fallout4_catalog_provenance_split_matches_the_2966_sweep() {
        let measured = FALLOUT4_BGS_CODE_OBJECT_METHODS
            .iter()
            .filter(|m| m.provenance == ScaleformKindProvenance::Measured)
            .count();
        let heuristic = FALLOUT4_BGS_CODE_OBJECT_METHODS
            .iter()
            .filter(|m| m.provenance == ScaleformKindProvenance::HeuristicNamePrefix)
            .count();
        assert_eq!(measured, 138, "the original F4CF-reconstructed entries");
        assert_eq!(heuristic, 131, "the #2966 corpus-sweep-added entries");
        assert_eq!(FALLOUT4_BGS_CODE_OBJECT_METHODS.len(), measured + heuristic);
    }

    /// #3103 — the Skyrim array is now the same two-provenance union the FO4
    /// one is: 74 entries whose `kind` is a measured protocol fact (SkyUI's
    /// fourth-argument callback test, read off its sources) plus the 68 the
    /// corpus sweep found that no source snapshot covers, classified by the
    /// same name-prefix heuristic #2966 used and marked as guesses.
    ///
    /// Pinned exactly rather than as a bound, so neither half can grow
    /// silently: a new `Measured` entry has to come with a protocol fact, and
    /// a new heuristic one has to be a sweep result.
    #[test]
    fn skyrim_catalog_provenance_split_matches_the_3103_sweep() {
        let measured = SKYRIM_SKYUI_METHODS
            .iter()
            .filter(|m| m.provenance == ScaleformKindProvenance::Measured)
            .count();
        let heuristic = SKYRIM_SKYUI_METHODS
            .iter()
            .filter(|m| m.provenance == ScaleformKindProvenance::HeuristicNamePrefix)
            .count();
        assert_eq!(measured, 74, "the original SkyUI-sourced entries");
        assert_eq!(heuristic, 68, "the #3103 corpus-sweep-added entries");
        assert_eq!(SKYRIM_SKYUI_METHODS.len(), measured + heuristic);
    }

    /// The heuristic half must stay a *minority* classification decision: the
    /// prefix set only recognises queries, so every name it does not match
    /// becomes a `Command`. Two of the 68 matched (`GetMouseButtonFor…`,
    /// `ShouldShowMod`), and `Cancel` is the camelCase-boundary case that must
    /// NOT match `Can*` — if that boundary rule is ever dropped, `Cancel`,
    /// `CharacterSelected` and friends silently become requests.
    #[test]
    fn the_skyrim_prefix_heuristic_respects_camel_case_boundaries() {
        let heuristic_requests: Vec<&str> = SKYRIM_SKYUI_METHODS
            .iter()
            .filter(|m| {
                m.provenance == ScaleformKindProvenance::HeuristicNamePrefix
                    && m.kind == ScaleformHostMethodKind::Request
            })
            .map(|m| m.name)
            .collect();
        assert_eq!(
            heuristic_requests,
            ["GetMouseButtonForSetDestination", "ShouldShowMod"],
            "only these two of the 68 sweep entries match Get*/Is*/Should*/Can*/get* \
             on a camelCase boundary"
        );
        let cancel = SKYRIM_SKYUI_METHODS
            .iter()
            .find(|m| m.name == "Cancel")
            .expect("Cancel is cataloged");
        assert_eq!(
            cancel.kind,
            ScaleformHostMethodKind::Command,
            "`Cancel` must not false-positive on the `Can*` prefix — the boundary \
             rule is what keeps it a command"
        );
    }
}
