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

// Calls through BGSCodeObj in F4CF/Interface's reconstructed vanilla Fallout
// 4 ActionScript sources. The project states that reconstructed areas are
// intended to remain as close as possible to the vanilla interface.
//
// BGSCodeObj is an AVM2 function table, not Skyrim's callback-bearing
// GameDelegate protocol, so every entry is a direct command here. Callers can
// still configure an immediate return value for functions used as queries.
//
// Source: https://github.com/F4CF/Interface/tree/master/Data/Interface/Source/Bethesda
static FALLOUT4_BGS_CODE_OBJECT_METHODS: &[ScaleformHostMethod] = &[
    ScaleformHostMethod::command("ActivateScrollSound"),
    ScaleformHostMethod::command("BackLevel"),
    ScaleformHostMethod::command("CanRepairSelectedItem"),
    ScaleformHostMethod::command("CenterMarkerRequest"),
    ScaleformHostMethod::command("CheckHardcoreModeFastTravel"),
    ScaleformHostMethod::command("CheckRequirements"),
    ScaleformHostMethod::command("ClearPlayerMarker"),
    ScaleformHostMethod::command("CloseMenu"),
    ScaleformHostMethod::command("ConfirmBuild"),
    ScaleformHostMethod::command("EndRotate3DItem"),
    ScaleformHostMethod::command("ExamineItem"),
    ScaleformHostMethod::command("FastTravel"),
    ScaleformHostMethod::command("FillModPartArray"),
    ScaleformHostMethod::command("GetButtonFromUserEvent"),
    ScaleformHostMethod::command("GetDisplayRate"),
    ScaleformHostMethod::command("GetHackingBoardCharHeight"),
    ScaleformHostMethod::command("GetHackingBoardCharWidth"),
    ScaleformHostMethod::command("GetNumGuesses"),
    ScaleformHostMethod::command("GetPerkInfoByRank"),
    ScaleformHostMethod::command("GetStartingListPosition"),
    ScaleformHostMethod::command("GetXPInfo"),
    ScaleformHostMethod::command("HideMenu"),
    ScaleformHostMethod::command("HolotapeActivate"),
    ScaleformHostMethod::command("IsSelectedItemEquipped"),
    ScaleformHostMethod::command("ItemDrop"),
    ScaleformHostMethod::command("ItemSelect"),
    ScaleformHostMethod::command("OnAcceptPress"),
    ScaleformHostMethod::command("OnAlternateButton"),
    ScaleformHostMethod::command("OnBuildFailed"),
    ScaleformHostMethod::command("OnMenuItemSelect"),
    ScaleformHostMethod::command("OnMobileSettingsLoaded"),
    ScaleformHostMethod::command("OnScrollingStarted"),
    ScaleformHostMethod::command("OnScrollingStopped"),
    ScaleformHostMethod::command("OnSpeechChallengeAnimComplete"),
    ScaleformHostMethod::command("PlayFocusSound"),
    ScaleformHostMethod::command("PlayPerkSound"),
    ScaleformHostMethod::command("PlaySmallTransition"),
    ScaleformHostMethod::command("PlaySound"),
    ScaleformHostMethod::command("PlayZoomSound"),
    ScaleformHostMethod::command("PopulatePipboyInfoObj"),
    ScaleformHostMethod::command("RefreshMapMarkers"),
    ScaleformHostMethod::command("RegisterComponents"),
    ScaleformHostMethod::command("RegisterMap"),
    ScaleformHostMethod::command("RegisterMovie"),
    ScaleformHostMethod::command("RegisterPerkGridComponents"),
    ScaleformHostMethod::command("RegisterTerminalElements"),
    ScaleformHostMethod::command("RemoveHighlight"),
    ScaleformHostMethod::command("RepairSelectedItem"),
    ScaleformHostMethod::command("RevertChanges"),
    ScaleformHostMethod::command("ScrapItem"),
    ScaleformHostMethod::command("SelectHackingWord"),
    ScaleformHostMethod::command("SelectItem"),
    ScaleformHostMethod::command("SelectPerk"),
    ScaleformHostMethod::command("SendTutorialEvent"),
    ScaleformHostMethod::command("SetItemSelectValuesForComponents"),
    ScaleformHostMethod::command("SetName"),
    ScaleformHostMethod::command("SetPlayerMarker"),
    ScaleformHostMethod::command("SetQuestActive"),
    ScaleformHostMethod::command("SetQuickkey"),
    ScaleformHostMethod::command("ShouldShowTagForSearchButton"),
    ScaleformHostMethod::command("ShowItem"),
    ScaleformHostMethod::command("ShowPerksMenu"),
    ScaleformHostMethod::command("ShowQuestOnMap"),
    ScaleformHostMethod::command("ShowWorkshopOnMap"),
    ScaleformHostMethod::command("SortItemList"),
    ScaleformHostMethod::command("StartBuildConfirm"),
    ScaleformHostMethod::command("StartItemSelection"),
    ScaleformHostMethod::command("StartRotate3DItem"),
    ScaleformHostMethod::command("StopPerkSound"),
    ScaleformHostMethod::command("SwitchBaseItem"),
    ScaleformHostMethod::command("SwitchMod"),
    ScaleformHostMethod::command("ToggleComponentFavorite"),
    ScaleformHostMethod::command("ToggleFavoriteMod"),
    ScaleformHostMethod::command("ToggleItemEquipped"),
    ScaleformHostMethod::command("ToggleRadioStationActiveStatus"),
    ScaleformHostMethod::command("UnregisterMap"),
    ScaleformHostMethod::command("UpdateRequirements"),
    ScaleformHostMethod::command("UseRadaway"),
    ScaleformHostMethod::command("UseStimpak"),
    ScaleformHostMethod::command("ValidateHackingWord"),
    ScaleformHostMethod::command("ZoomIn"),
    ScaleformHostMethod::command("ZoomOut"),
    ScaleformHostMethod::command("closeMenu"),
    ScaleformHostMethod::command("confirmInvest"),
    ScaleformHostMethod::command("executeCommand"),
    ScaleformHostMethod::command("exitMenu"),
    ScaleformHostMethod::command("functiononGPSModeButtonClicked"),
    ScaleformHostMethod::command("getButtonFromUserEvent"),
    ScaleformHostMethod::command("getItemValue"),
    ScaleformHostMethod::command("getScrollSpeed"),
    ScaleformHostMethod::command("getSelectedItemEquippable"),
    ScaleformHostMethod::command("getSelectedItemEquipped"),
    ScaleformHostMethod::command("inspectItem"),
    ScaleformHostMethod::command("onAcceptPress"),
    ScaleformHostMethod::command("onBackButtonHandled"),
    ScaleformHostMethod::command("onButtonPress"),
    ScaleformHostMethod::command("onButtonRelease"),
    ScaleformHostMethod::command("onCancelPress"),
    ScaleformHostMethod::command("onComponentViewToggle"),
    ScaleformHostMethod::command("onFadeDone"),
    ScaleformHostMethod::command("onGridAddedToStage"),
    ScaleformHostMethod::command("onHideComplete"),
    ScaleformHostMethod::command("onIntroAnimComplete"),
    ScaleformHostMethod::command("onInvItemSelection"),
    ScaleformHostMethod::command("onManualModeButtonClicked"),
    ScaleformHostMethod::command("onMenuLoadComplete"),
    ScaleformHostMethod::command("onModalOpen"),
    ScaleformHostMethod::command("onNewPage"),
    ScaleformHostMethod::command("onNewTab"),
    ScaleformHostMethod::command("onPerksTabClose"),
    ScaleformHostMethod::command("onPerksTabOpen"),
    ScaleformHostMethod::command("onQuestSelection"),
    ScaleformHostMethod::command("onScanButtonClicked"),
    ScaleformHostMethod::command("onShowHotKeys"),
    ScaleformHostMethod::command("onSwitchBetweenWorldLocalMap"),
    ScaleformHostMethod::command("registerObjects"),
    ScaleformHostMethod::command("requestCredits"),
    ScaleformHostMethod::command("sendXButton"),
    ScaleformHostMethod::command("sendYButton"),
    ScaleformHostMethod::command("show3D"),
    ScaleformHostMethod::command("sortItems"),
    ScaleformHostMethod::command("takeAllItems"),
    ScaleformHostMethod::command("toggleMovementToDirectional"),
    ScaleformHostMethod::command("toggleSelectedItemEquipped"),
    ScaleformHostMethod::command("transferItem"),
    ScaleformHostMethod::command("updateItem3D"),
    ScaleformHostMethod::command("updateItemPickpocketInfo"),
    ScaleformHostMethod::command("updateSortButtonLabel"),
    ScaleformHostMethod::command("useQuickkey"),
];
