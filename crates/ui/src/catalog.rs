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

/// One method in a profile's known host surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScaleformHostMethod {
    pub name: &'static str,
    pub kind: ScaleformHostMethodKind,
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
        }
    }

    const fn request(name: &'static str) -> Self {
        Self {
            name,
            kind: ScaleformHostMethodKind::Request,
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
static SKYRIM_SKYUI_METHODS: &[ScaleformHostMethod] = &[
    ScaleformHostMethod::command("AuxButtonPress"),
    ScaleformHostMethod::request("CalculateCharge"),
    ScaleformHostMethod::request("CanFadeItemInfo"),
    ScaleformHostMethod::request("CheckForMouseEquip"),
    ScaleformHostMethod::command("ChooseItem"),
    ScaleformHostMethod::command("ClickCallback"),
    ScaleformHostMethod::command("CloseMenu"),
    ScaleformHostMethod::command("CloseTweenMenu"),
    ScaleformHostMethod::command("CraftButtonPress"),
    ScaleformHostMethod::command("CraftSelectedItem"),
    ScaleformHostMethod::command("CurrentLocationCallback"),
    ScaleformHostMethod::command("DeleteSave"),
    ScaleformHostMethod::command("DisabledItemSelect"),
    ScaleformHostMethod::command("EndItemRename"),
    ScaleformHostMethod::command("EquipItem"),
    ScaleformHostMethod::command("FadeDone"),
    ScaleformHostMethod::request("GetButtonFromUserEvent"),
    ScaleformHostMethod::request("GetRawDealWarningString"),
    ScaleformHostMethod::command("IsOKtoLoad"),
    ScaleformHostMethod::command("ItemCardListCallback"),
    ScaleformHostMethod::command("ItemDrop"),
    ScaleformHostMethod::command("ItemSelect"),
    ScaleformHostMethod::command("ItemTransfer"),
    ScaleformHostMethod::command("LOAD"),
    ScaleformHostMethod::command("LoadGame"),
    ScaleformHostMethod::command("MarkerClick"),
    ScaleformHostMethod::command("OpenJournalCallback"),
    ScaleformHostMethod::command("OpenKinectTuner"),
    ScaleformHostMethod::command("OptionChange"),
    ScaleformHostMethod::command("PlaySound"),
    ScaleformHostMethod::command("PopulateHelpTopics"),
    ScaleformHostMethod::command("PrepSaveGameScreenshot"),
    ScaleformHostMethod::command("QuantitySliderOpen"),
    ScaleformHostMethod::command("QuitToDesktop"),
    ScaleformHostMethod::command("QuitToMainMenu"),
    ScaleformHostMethod::command("RequestAudioOptions"),
    ScaleformHostMethod::command("RequestDisplayOptions"),
    ScaleformHostMethod::command("RequestGameplayOptions"),
    ScaleformHostMethod::command("RequestHelpText"),
    ScaleformHostMethod::command("RequestInputMappings"),
    ScaleformHostMethod::request("RequestIsOnPC"),
    ScaleformHostMethod::request("RequestItemCardInfo"),
    ScaleformHostMethod::command("RequestObjectivesData"),
    ScaleformHostMethod::request("RequestPlayerInfo"),
    ScaleformHostMethod::request("RequestQuestsData"),
    ScaleformHostMethod::command("ResetControlsToDefaults"),
    ScaleformHostMethod::command("SAVE"),
    ScaleformHostMethod::command("SaveControls"),
    ScaleformHostMethod::command("SaveGame"),
    ScaleformHostMethod::command("SaveIndices"),
    ScaleformHostMethod::command("SaveSettings"),
    ScaleformHostMethod::command("SetLocalMapExtents"),
    ScaleformHostMethod::command("SetSaveDisabled"),
    ScaleformHostMethod::command("SetSelectedCategory"),
    ScaleformHostMethod::command("SetSelectedItem"),
    ScaleformHostMethod::command("SetVersionText"),
    ScaleformHostMethod::request("ShouldShowKinectTunerOption"),
    ScaleformHostMethod::command("ShowItem3D"),
    ScaleformHostMethod::command("ShowShoutFail"),
    ScaleformHostMethod::command("ShowSoulGemList"),
    ScaleformHostMethod::command("ShowTargetOnMap"),
    ScaleformHostMethod::command("ShowTweenMenu"),
    ScaleformHostMethod::command("SliderClose"),
    ScaleformHostMethod::command("StartMouseRotation"),
    ScaleformHostMethod::command("StartRemapMode"),
    ScaleformHostMethod::command("StopMouseRotation"),
    ScaleformHostMethod::command("TakeAllItems"),
    ScaleformHostMethod::command("ToggleMapCallback"),
    ScaleformHostMethod::request("ToggleQuestActiveStatus"),
    ScaleformHostMethod::command("ToggleShowMiscObjectives"),
    ScaleformHostMethod::command("UpdateItem3D"),
    ScaleformHostMethod::command("ZoomItemModel"),
    ScaleformHostMethod::command("buttonPress"),
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
    ScaleformHostMethod::command("Accept"),
    ScaleformHostMethod::command("ActivateScrollSound"),
    ScaleformHostMethod::command("AreModsLoaded"),
    ScaleformHostMethod::command("BackLevel"),
    ScaleformHostMethod::command("CClubBlockedByBnet"),
    ScaleformHostMethod::command("CClubBlockedByPermissions"),
    ScaleformHostMethod::request("CanRepairSelectedItem"),
    ScaleformHostMethod::command("Cancel"),
    ScaleformHostMethod::command("CancelPlayback"),
    ScaleformHostMethod::command("CenterMarkerRequest"),
    ScaleformHostMethod::command("ChangeBeard"),
    ScaleformHostMethod::command("ChangeCharacterPreset"),
    ScaleformHostMethod::command("ChangeColor"),
    ScaleformHostMethod::command("ChangeHairColor"),
    ScaleformHostMethod::command("ChangeHairStyle"),
    ScaleformHostMethod::command("ChangePreset"),
    ScaleformHostMethod::command("ChangePresetIntensity"),
    ScaleformHostMethod::command("ChangeSex"),
    ScaleformHostMethod::command("CheckHardcoreModeFastTravel"),
    ScaleformHostMethod::command("CheckRequirements"),
    ScaleformHostMethod::command("ClearBoneRegionTint"),
    ScaleformHostMethod::command("ClearDetails"),
    ScaleformHostMethod::command("ClearPickData"),
    ScaleformHostMethod::command("ClearPlayerMarker"),
    ScaleformHostMethod::command("ClearTemporaryDetail"),
    ScaleformHostMethod::command("CloseMenu"),
    ScaleformHostMethod::command("ConfirmAndCloseMenu"),
    ScaleformHostMethod::command("ConfirmBuild"),
    ScaleformHostMethod::command("ContinueGame"),
    ScaleformHostMethod::command("CreateSavePoint"),
    ScaleformHostMethod::command("CreateUndoPoint"),
    ScaleformHostMethod::command("CycleBodyPart"),
    ScaleformHostMethod::command("CycleTarget"),
    ScaleformHostMethod::command("DeleteDLC"),
    ScaleformHostMethod::command("DeleteSave"),
    ScaleformHostMethod::command("DoQuicksave"),
    ScaleformHostMethod::command("DownloadAll"),
    ScaleformHostMethod::command("DownloadCreation"),
    ScaleformHostMethod::command("EndBodyEdit"),
    ScaleformHostMethod::command("EndRotate3DItem"),
    ScaleformHostMethod::command("EnterCreationClub"),
    ScaleformHostMethod::command("EnterCreditsScreen"),
    ScaleformHostMethod::command("EnterCreditsScreenFromExpandedMenu"),
    ScaleformHostMethod::command("EnterDetailsScreen"),
    ScaleformHostMethod::command("ExamineItem"),
    ScaleformHostMethod::command("ExecuteCritical"),
    ScaleformHostMethod::command("FastTravel"),
    ScaleformHostMethod::command("FillModPartArray"),
    ScaleformHostMethod::command("FinishLoadGame"),
    ScaleformHostMethod::command("FinishSaveGame"),
    ScaleformHostMethod::request("GetButtonFromUserEvent"),
    ScaleformHostMethod::request("GetDetailColor"),
    ScaleformHostMethod::request("GetDetailColorCount"),
    ScaleformHostMethod::request("GetDetailIntensity"),
    ScaleformHostMethod::request("GetDisplayRate"),
    ScaleformHostMethod::request("GetExtraGroupName"),
    ScaleformHostMethod::request("GetFeatureData"),
    ScaleformHostMethod::request("GetHackingBoardCharHeight"),
    ScaleformHostMethod::request("GetHackingBoardCharWidth"),
    ScaleformHostMethod::request("GetHasDetailsApplied"),
    ScaleformHostMethod::request("GetHasInstalledContent"),
    ScaleformHostMethod::request("GetHasSavedGames"),
    ScaleformHostMethod::request("GetLastCharacterPreset"),
    ScaleformHostMethod::request("GetNumGuesses"),
    ScaleformHostMethod::request("GetPerkInfoByRank"),
    ScaleformHostMethod::request("GetShowBethesdaNetOption"),
    ScaleformHostMethod::request("GetShowCreationClubOption"),
    ScaleformHostMethod::request("GetStartingListPosition"),
    ScaleformHostMethod::request("GetXPInfo"),
    ScaleformHostMethod::command("HideMenu"),
    ScaleformHostMethod::command("HighlightBoneRegion"),
    ScaleformHostMethod::command("HolotapeActivate"),
    ScaleformHostMethod::command("InitCreationClub"),
    ScaleformHostMethod::command("InitLoginObject"),
    ScaleformHostMethod::command("InitialPopulateLoadList"),
    ScaleformHostMethod::request("IsDLCReady"),
    ScaleformHostMethod::request("IsMainMenuReady"),
    ScaleformHostMethod::request("IsSelectedItemEquipped"),
    ScaleformHostMethod::command("ItemDrop"),
    ScaleformHostMethod::command("ItemSelect"),
    ScaleformHostMethod::command("Land"),
    ScaleformHostMethod::command("ModsBlockedByBnet"),
    ScaleformHostMethod::command("NotifyForWittyBanter"),
    ScaleformHostMethod::command("OnAcceptPress"),
    ScaleformHostMethod::command("OnAlternateButton"),
    ScaleformHostMethod::command("OnAnimateOutComplete"),
    ScaleformHostMethod::command("OnBuildFailed"),
    ScaleformHostMethod::command("OnMenuItemSelect"),
    ScaleformHostMethod::command("OnMobileSettingsLoaded"),
    ScaleformHostMethod::command("OnPS5DataTransfer"),
    ScaleformHostMethod::command("OnScrollingStarted"),
    ScaleformHostMethod::command("OnScrollingStopped"),
    ScaleformHostMethod::command("OnSpeechChallengeAnimComplete"),
    ScaleformHostMethod::command("PlayCancelSound"),
    ScaleformHostMethod::command("PlayFocusSound"),
    ScaleformHostMethod::command("PlayOKSound"),
    ScaleformHostMethod::command("PlayPerkSound"),
    ScaleformHostMethod::command("PlayPopupSound"),
    ScaleformHostMethod::command("PlaySmallTransition"),
    ScaleformHostMethod::command("PlaySound"),
    ScaleformHostMethod::command("PlayTabLeftSound"),
    ScaleformHostMethod::command("PlayTabRightSound"),
    ScaleformHostMethod::command("PlayZoomSound"),
    ScaleformHostMethod::command("PopulateCharacterList"),
    ScaleformHostMethod::command("PopulateDLCList"),
    ScaleformHostMethod::command("PopulateHelpTopics"),
    ScaleformHostMethod::command("PopulateInstalledContentTopics"),
    ScaleformHostMethod::command("PopulateLoadList"),
    ScaleformHostMethod::command("PopulatePipboyInfoObj"),
    ScaleformHostMethod::command("PopulateSaveList"),
    ScaleformHostMethod::command("PurchaseDLC"),
    ScaleformHostMethod::command("PurchaseMod"),
    ScaleformHostMethod::command("QueueAction"),
    ScaleformHostMethod::command("RefreshMapMarkers"),
    ScaleformHostMethod::command("RegisterComponents"),
    ScaleformHostMethod::command("RegisterMap"),
    ScaleformHostMethod::command("RegisterMovie"),
    ScaleformHostMethod::command("RegisterPerkGridComponents"),
    ScaleformHostMethod::command("RegisterSaveLoadPanel"),
    ScaleformHostMethod::command("RegisterTerminalElements"),
    ScaleformHostMethod::command("RemoveHighlight"),
    ScaleformHostMethod::command("RepairSelectedItem"),
    ScaleformHostMethod::command("RequestAudioOptions"),
    ScaleformHostMethod::command("RequestDisplayOptions"),
    ScaleformHostMethod::command("RequestGameplayOptions"),
    ScaleformHostMethod::command("RequestHelpText"),
    ScaleformHostMethod::command("RequestHelpTitle"),
    ScaleformHostMethod::command("RequestInputMappings"),
    ScaleformHostMethod::command("RequestInstalledContentText"),
    ScaleformHostMethod::command("RequestInstalledContentTitle"),
    ScaleformHostMethod::command("RequestRefreshInstallProgress"),
    ScaleformHostMethod::command("ResetControlsToDefaults"),
    ScaleformHostMethod::command("ReturnFromDLC"),
    ScaleformHostMethod::command("RevertChanges"),
    ScaleformHostMethod::command("SaveSettings"),
    ScaleformHostMethod::command("ScrapItem"),
    ScaleformHostMethod::command("SelectHackingWord"),
    ScaleformHostMethod::command("SelectItem"),
    ScaleformHostMethod::command("SelectPerk"),
    ScaleformHostMethod::command("SendTutorialEvent"),
    ScaleformHostMethod::command("SetBackgroundVisible"),
    ScaleformHostMethod::command("SetBumpersRepeat"),
    ScaleformHostMethod::command("SetCurrentCharacter"),
    ScaleformHostMethod::command("SetDetailColor"),
    ScaleformHostMethod::command("SetDetailIntensity"),
    ScaleformHostMethod::command("SetHairHighlight"),
    ScaleformHostMethod::command("SetItemSelectValuesForComponents"),
    ScaleformHostMethod::command("SetName"),
    ScaleformHostMethod::command("SetPlayerMarker"),
    ScaleformHostMethod::command("SetQuestActive"),
    ScaleformHostMethod::command("SetQuickkey"),
    ScaleformHostMethod::request("ShouldShowTagForSearchButton"),
    ScaleformHostMethod::command("ShowChangeUser"),
    ScaleformHostMethod::command("ShowContinueSecondPanel"),
    ScaleformHostMethod::command("ShowCreditMenu"),
    ScaleformHostMethod::command("ShowItem"),
    ScaleformHostMethod::command("ShowPerksMenu"),
    ScaleformHostMethod::command("ShowPlatformHelp"),
    ScaleformHostMethod::command("ShowQuestOnMap"),
    ScaleformHostMethod::command("ShowWorkshopOnMap"),
    ScaleformHostMethod::command("SortItemList"),
    ScaleformHostMethod::command("StartBodyEdit"),
    ScaleformHostMethod::command("StartBuildConfirm"),
    ScaleformHostMethod::command("StartItemSelection"),
    ScaleformHostMethod::command("StartNewGame"),
    ScaleformHostMethod::command("StartPlayback"),
    ScaleformHostMethod::command("StartRemapMode"),
    ScaleformHostMethod::command("StartRotate3DItem"),
    ScaleformHostMethod::command("StartWaiting"),
    ScaleformHostMethod::command("StopPerkSound"),
    ScaleformHostMethod::command("SwitchBaseItem"),
    ScaleformHostMethod::command("SwitchMod"),
    ScaleformHostMethod::command("ToggleComponentFavorite"),
    ScaleformHostMethod::command("ToggleFavoriteMod"),
    ScaleformHostMethod::command("ToggleItemEquipped"),
    ScaleformHostMethod::command("ToggleRadioStationActiveStatus"),
    ScaleformHostMethod::command("UndoLastAction"),
    ScaleformHostMethod::command("UnregisterMap"),
    ScaleformHostMethod::command("UpdateDLC"),
    ScaleformHostMethod::command("UpdateRequirements"),
    ScaleformHostMethod::command("UpsellPressed"),
    ScaleformHostMethod::command("UseRadaway"),
    ScaleformHostMethod::command("UseStimpak"),
    ScaleformHostMethod::command("ValidateHackingWord"),
    ScaleformHostMethod::command("WeightPointChange"),
    ScaleformHostMethod::command("ZoomIn"),
    ScaleformHostMethod::command("ZoomOut"),
    ScaleformHostMethod::command("attemptCloseManager"),
    ScaleformHostMethod::command("closeHolotape"),
    ScaleformHostMethod::command("closeMenu"),
    ScaleformHostMethod::command("confirmCloseManager"),
    ScaleformHostMethod::command("confirmInvest"),
    ScaleformHostMethod::command("confirmResetPoints"),
    ScaleformHostMethod::command("executeCommand"),
    ScaleformHostMethod::command("exitMenu"),
    ScaleformHostMethod::request("getButtonFromUserEvent"),
    ScaleformHostMethod::request("getHighscore"),
    ScaleformHostMethod::request("getItemValue"),
    ScaleformHostMethod::request("getSaveData"),
    ScaleformHostMethod::request("getScrollSpeed"),
    ScaleformHostMethod::request("getSelectedItemEquippable"),
    ScaleformHostMethod::request("getSelectedItemEquipped"),
    ScaleformHostMethod::request("getTextReplaceID"),
    ScaleformHostMethod::request("getTextReplaceValue"),
    ScaleformHostMethod::command("initMenu"),
    ScaleformHostMethod::command("inspectItem"),
    ScaleformHostMethod::command("modValue"),
    ScaleformHostMethod::command("notifyScripts"),
    ScaleformHostMethod::command("onAcceptPress"),
    ScaleformHostMethod::command("onBackButtonHandled"),
    ScaleformHostMethod::command("onButtonPress"),
    ScaleformHostMethod::command("onButtonRelease"),
    ScaleformHostMethod::command("onCancel"),
    ScaleformHostMethod::command("onCancelPress"),
    ScaleformHostMethod::command("onComponentViewToggle"),
    ScaleformHostMethod::command("onContinuePress"),
    ScaleformHostMethod::command("onDisabledLoadPress"),
    ScaleformHostMethod::command("onFadeDone"),
    ScaleformHostMethod::command("onGPSModeButtonClicked"),
    ScaleformHostMethod::command("onGridAddedToStage"),
    ScaleformHostMethod::command("onHUDColorUpdate"),
    ScaleformHostMethod::command("onHideComplete"),
    ScaleformHostMethod::command("onIntroAnimComplete"),
    ScaleformHostMethod::command("onInvItemSelection"),
    ScaleformHostMethod::command("onLocationConfirm"),
    ScaleformHostMethod::command("onManualModeButtonClicked"),
    ScaleformHostMethod::command("onMenuLoadComplete"),
    ScaleformHostMethod::command("onModalOpen"),
    ScaleformHostMethod::command("onNewPage"),
    ScaleformHostMethod::command("onNewPress"),
    ScaleformHostMethod::command("onNewTab"),
    ScaleformHostMethod::command("onPerksTabClose"),
    ScaleformHostMethod::command("onPerksTabOpen"),
    ScaleformHostMethod::command("onPipboyColorUpdate"),
    ScaleformHostMethod::command("onQuestSelection"),
    ScaleformHostMethod::command("onQuitPress"),
    ScaleformHostMethod::command("onQuitToDesktopPress"),
    ScaleformHostMethod::command("onScanButtonClicked"),
    ScaleformHostMethod::command("onSettingsValueChange"),
    ScaleformHostMethod::command("onShowHotKeys"),
    ScaleformHostMethod::command("onSwitchBetweenWorldLocalMap"),
    ScaleformHostMethod::command("pauseRegisteredSound"),
    ScaleformHostMethod::command("playActionAnim"),
    ScaleformHostMethod::command("playLeftAnim"),
    ScaleformHostMethod::command("playRegisteredSound"),
    ScaleformHostMethod::command("playRightAnim"),
    ScaleformHostMethod::command("playSound"),
    ScaleformHostMethod::command("populateCaravanList"),
    ScaleformHostMethod::command("registerObjects"),
    ScaleformHostMethod::command("registerSound"),
    ScaleformHostMethod::command("requestCredits"),
    ScaleformHostMethod::command("requestLoadingText"),
    ScaleformHostMethod::command("sendXButton"),
    ScaleformHostMethod::command("sendYButton"),
    ScaleformHostMethod::command("setHighscore"),
    ScaleformHostMethod::command("setSaveData"),
    ScaleformHostMethod::command("show3D"),
    ScaleformHostMethod::command("sortItems"),
    ScaleformHostMethod::command("startEditText"),
    ScaleformHostMethod::command("stopRegisteredSound"),
    ScaleformHostMethod::command("takeAllItems"),
    ScaleformHostMethod::command("toggleMovementToDirectional"),
    ScaleformHostMethod::command("toggleSelectedItemEquipped"),
    ScaleformHostMethod::command("transferItem"),
    ScaleformHostMethod::command("tryCloseMenu"),
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
}
