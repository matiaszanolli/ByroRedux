#!/usr/bin/env python3
# Scratch generator for the two HUD feed TSVs (not part of the repo).
# Verifies each EXE string key against the extracted exe string tables at
# generation time so the evidence column cannot claim a string that is absent.
import sys

D = "/tmp/claude-1000/-mnt-data-src-gamebyro-redux/474aab05-47bb-46c1-a8c9-338f7d6546ca/scratchpad/m48p0/B/"
OUT = "/mnt/data/src/gamebyro-redux/docs/data/game-ui/"

sky_exe = set(l.rstrip("\n") for l in open(D + "skyrimse_strings.txt", errors="replace"))
fo4_exe = set(l.rstrip("\n") for l in open(D + "fo4_exe_strings.txt", errors="replace"))

CLSSE = "https://github.com/CharmedBaryon/CommonLibSSE-NG/blob/b93280e832f263dbef44e44cbe2936622a02f91a/include/RE/"
F4CF = "https://github.com/F4CF/Interface/blob/f8b6fcfe35ca01667b8fd76de2bba53113b2abd7/Data/Interface/Source/Bethesda/UI/HUDMenu/"

HDR = ["path", "member_kind", "engine_fed", "engine_fed_basis", "movie_sets_itself", "evidence", "proposed_canonical_source", "compass_related", "notes"]


def exe_ev(exe, name, keys):
    if not keys:
        return ""
    present = [k for k in keys if k in exe]
    absent = [k for k in keys if k not in exe]
    s = f"; {name} `strings -a -n5` exact line present: {','.join(present) or '-'}"
    if absent:
        s += f" / absent: {','.join(absent)}"
    return s


def emit(fname, exe, exename, rows):
    lines = ["\t".join(HDR)]
    for r in rows:
        (path, kind, fed, sets, bc, keys, ext, canon, comp, notes) = r
        assert kind in ("meter", "compass", "text", "flag", "visibility", "other"), (path, kind)
        assert fed in ("yes", "no", "unclear"), path
        assert sets in ("yes", "no"), path
        assert comp in ("yes", "no"), path
        present = [k for k in keys if k in exe]
        if fed == "yes":
            if exe is sky_exe:
                basis = ("native-strings" + ("+CommonLibSSE" if "CommonLibSSE" in ext else "")) if present else ("CommonLibSSE" if "CommonLibSSE" in ext else "bytecode-only")
            else:
                basis = "inferred:no-movie-writer+exe-strings" if present else "inferred:no-movie-writer"
        else:
            basis = ""
        ev = "bytecode: " + bc + exe_ev(exe, exename, keys)
        if ext:
            ev += "; external: " + ext
        for f in (path, ev, canon, notes):
            assert "\t" not in f and "\n" not in f, (path, f)
        lines.append("\t".join([path, kind, fed, basis, sets, ev, canon, comp, notes]))
    open(OUT + fname, "w").write("\n".join(lines) + "\n")
    print(fname, len(lines) - 1, "rows")


# ───────────────────────── Skyrim SE ─────────────────────────
T = "hud_feed_paths skyrim dump"
H = "HUDMenu class (DoInitAction sprite 889; Object.registerClass('HUDMovie', HUDMenu) in DoInitAction sprite 862)"
VIT = "ActorValues of PlayerEntity via PlayerVitals::resolved(editor id) (byroredux/src/scaleform_hud.rs bar_fractions; PlayerVitals in byroredux/src/inventory/catalog.rs:205)"
HDG = "ActiveCamera forward -> atan2(f.x,-f.z) degrees, HudControl.heading override (byroredux/src/hud.rs:637, byroredux/src/scaleform_hud.rs:384)"
NONE = "none today"
OBJ = CLSSE + "H/HUDObject.h"
MNU = CLSSE + "H/HUDMenu.h"
MTR = CLSSE + "H/HUDMeter.h"

def cb(name):
    return f"movie also registers GameDelegate callback key '{name}' (InitExtensions addCallBack)"

sky = []
A = sky.append

A(("_root.HUDMovieBaseInstance", "other", "yes", "no",
   f"{T}: root timeline places char 862 as 'HUDMovieBaseInstance' (`skyrim tree`); {H}",
   [], MNU + " HUDMenu::RUNTIME_DATA.root comment '_level0.HUDMovieBaseInstance'",
   "n/a (path-resolution root, GUI-PATH-001)", "no",
   "Every engine-fed member below hangs off this instance; the native menu stores it as its GFx root value"))
A(("_root.InitExtensions()", "other", "yes", "no",
   f"{T}: root frame0 DoAction defines it; body only calls HUDMovieBaseInstance.InitExtensions(); no movie code calls _root.InitExtensions",
   ["_root.InitExtensions"], "", "n/a (lifecycle)", "no",
   "Engine calls it once after load; it runs the class InitExtensions that registers the callback table"))
A(("_root.SetPlatform(aPlatform, abPS3Switch)", "other", "yes", "no",
   f"{T}: root frame0 DoAction defines it, forwards to HUDMovieBaseInstance.SetPlatform; no movie caller",
   ["_root.SetPlatform"], "", NONE + " (input-device family: keyboard / pad type)", "no",
   "aPlatform picks the button-art set; value mapping (CONTROLLER_* constants 3..6 seen in the class) not established for 0..2"))
A(("_root.CurrentTime", "other", "unclear", "yes",
   f"{T}: root frame0 DoAction `var CurrentTime = -1`; no movie code reads or writes it again",
   ["CurrentTime"], "", NONE, "no",
   "Declared and never read by the movie; the exe string sits in the HUD string cluster but its use is not established"))
A(("_root.HUDMovieBaseInstance.InitExtensions()", "other", "no", "yes",
   f"{T}: HUDMenu.prototype.InitExtensions; called by _root.InitExtensions; body issues gfx.io.GameDelegate.addCallBack for 26 keys",
   [], "", "n/a (lifecycle)", "no",
   "Registers the callback table (see GameDelegate rows) and reads TopLeftRef/BottomRightRef layout anchors"))
A(("_root.HUDMovieBaseInstance.RegisterComponents()", "other", "yes", "no",
   f"{T}: HUDMenu.prototype.RegisterComponents calls GameDelegate.call('RegisterHUDComponents',[this,HudElements,QuestUpdateBaseInstance,EnemyHealthMeter,StealthMeterInstance,StealthMeterInstance.SneakAnimInstance,EnemyHealth_mc.BracketsInstance,EnemyHealth_mc.BracketsInstance.RolloverNameInstance,StealthMeterInstance.SneakTextHolder,StealthMeterInstance.SneakTextHolder.SneakTextClip.SneakTextInstance]); no movie caller",
   ["HUDMovieBaseInstance.RegisterComponents", "RegisterHUDComponents"], OBJ + " HudComponents enum (kThis,kHudElements,kQuestUpdateBaseInstance,kEnemyHealthMeter,kStealthMeterInstance,kSneakAnimInstance,kBracketsInstance,kRolloverNameInstance,kSneakTextHolder,kSneakTextInstance) matches the array order; exe has the full path string 'HUDMovieBaseInstance.RegisterComponents'",
   "n/a (the ten objects it hands over are the native-held component handles; rows below)", "no",
   "Exe check: line 'HUDMovieBaseInstance.RegisterComponents' present in SkyrimSE.exe strings"))

# Vitals
for stat, fn, avname in (("Health", "SetHealthMeterPercent", "Health"), ("Magicka", "SetMagickaMeterPercent", "Magicka"), ("Stamina", "SetStaminaMeterPercent", "Stamina")):
    A((f"_root.HUDMovieBaseInstance.{fn}(aPercent, abForce)", "meter", "yes", "no",
       f"{T}: HUDMenu.prototype.{fn}; `skyrim fed`: only in-movie caller is the survival penalty setter re-calling it with the cached last value; {cb(fn)}",
       [fn], MTR + " (HUDMeter.setPctName BSFixedString + HUDMenu::RUNTIME_DATA health/stamina/magicka ActorValueMeter*) ; https://git.eddoursul.win/enderal/enderalse/commit/85c1cc1b0ca2dd77f47855d2cc69c414a5ef63f7 (mod, not vanilla: native code calling uiMovie->Invoke('HUDMovieBaseInstance.HealthMeterAnim.PlayForward') on the same instance)",
       f"{VIT} -> editor id '{avname}', value*100", "no",
       "aPercent 0..100 (Components.Meter.SetPercent clamps with Math.min(100,Math.max(p,0))); abForce=true snaps, false animates via SetTargetPercent; stat-vs-max composition is the canonical layer's"))
A(("_root.HUDMovieBaseInstance.SetShoutMeterPercent(aPercent, abForce)", "meter", "yes", "no",
   f"{T}: HUDMenu.prototype.SetShoutMeterPercent -> ShoutMeter.SetPercent; no movie caller; {cb('SetShoutMeterPercent')}",
   ["SetShoutMeterPercent"], CLSSE + "S/ShoutMeter.h (ShoutMeter : HUDMeter, cooldown in seconds) ; " + MNU + " runtime shout member",
   NONE + " (no shout cooldown component; nearest: none)", "no", "aPercent 0..100; FlashShoutMeter is the separate 'cooldown over' flash"))
A(("_root.HUDMovieBaseInstance.FlashShoutMeter()", "flag", "yes", "no",
   f"{T}: HUDMenu.prototype.FlashShoutMeter -> ShoutMeter.FlashMeter; no movie caller; {cb('FlashShoutMeter')}",
   ["FlashShoutMeter"], "", NONE, "no", "Event, not a level"))
A(("_root.HUDMovieBaseInstance.SetChargeMeterPercent(aPercent, abForce, abLeftHand, abShow)", "meter", "yes", "no",
   f"{T}: HUDMenu.prototype.SetChargeMeterPercent drives Left/RightChargeMeter (Components.Meter); no movie caller; {cb('SetChargeMeterPercent')}",
   ["SetChargeMeterPercent"], CLSSE + "H/HUDChargeMeter.h (HUDObject subclass, charge pct fields 100.0)",
   NONE + " (weapon / spell charge)", "no", "abShow=false parks the clip on frame 1"))
A(("_root.HUDMovieBaseInstance.FadeOutChargeMeters()", "visibility", "yes", "no",
   f"{T}: HUDMenu.prototype.FadeOutChargeMeters; no movie caller; {cb('FadeOutChargeMeters')}",
   ["FadeOutChargeMeters"], "", NONE, "no", ""))
A(("_root.HUDMovieBaseInstance.FadeOutStamina(aPercent)", "visibility", "yes", "no",
   f"{T}: HUDMenu.prototype.FadeOutStamina (also sets StaminaMeter.CurrentPercent/TargetPercent); no movie caller; {cb('FadeOutStamina')}",
   ["FadeOutStamina"], MTR + " (HUDMeter.fadeOutName)", NONE + " (meter idle-fade policy)", "no", "The meters fade out when full and idle; Redux has no equivalent state"))
A(("_root.HUDMovieBaseInstance.StartMagickaBlinking()", "flag", "yes", "no",
   f"{T}: HUDMenu.prototype.StartMagickaBlinking; no movie caller; the callback key registered for it is 'StartMagickaMeterBlinking', a different string",
   ["StartMagickaBlinking", "StartMagickaMeterBlinking"], MTR + " (HUDMeter.setBlinkingName)", NONE + " (insufficient-resource flash)", "no",
   "Exe carries the method name, not the callback key: native reaches it by method name on the instance, not through the callback table"))
A(("_root.HUDMovieBaseInstance.StartStaminaBlinking()", "flag", "yes", "no",
   f"{T}: HUDMenu.prototype.StartStaminaBlinking; no movie caller; callback key is 'StartStaminaMeterBlinking'",
   ["StartStaminaBlinking", "StartStaminaMeterBlinking"], MTR + " (HUDMeter.setBlinkingName)", NONE, "no", "Same note as StartMagickaBlinking"))
A(("_root.HUDMovieBaseInstance.ShowSurvivalElements(abShow, aUpdateData, abForce)", "other", "yes", "no",
   f"{T}: HUDMenu.prototype.ShowSurvivalElements reads aUpdateData.aTemperatureLevel/aColdPenaltyPercent/aHungerPenaltyPercent/aExhaustionPenaltyPercent; no movie caller; NOT in the callback table (path-invoke only)",
   ["ShowSurvivalElements", "aTemperatureLevel", "aColdPenaltyPercent", "aHungerPenaltyPercent", "aExhaustionPenaltyPercent"], "",
   NONE + " (Survival mode not modelled)", "yes",
   "aUpdateData is an object with those four fields; drives the compass temperature gauge and penalty overlays on the three meters"))
for fn, what in (("SetCompassTemperature", "compass temperature gauge frame"), ("SetColdPenaltyMeter", "health penalty overlay"), ("SetHungerPenaltyMeter", "stamina penalty overlay"), ("SetExhaustionPenaltyMeter", "magicka penalty overlay")):
    A((f"_root.HUDMovieBaseInstance.{fn}", "meter" if "Penalty" in fn else "compass", "no", "yes",
       f"{T}: HUDMenu.prototype.{fn}; called only from ShowSurvivalElements (movie-internal)",
       [fn], "", "n/a (reached through ShowSurvivalElements)", "yes" if fn == "SetCompassTemperature" else "no",
       what + ("; the movie also registers it as a GameDelegate callback key but the exe does not carry the name" if fn == "SetCompassTemperature" else "")))

# Enemy health
A(("_root.HUDMovieBaseInstance.EnemyHealthMeter.SetPercent(aPercent)", "meter", "yes", "no",
   f"{T}: EnemyHealthMeter = new Components.Meter(EnemyHealth_mc) (HUDMenu ctor); handed to native as RegisterHUDComponents arg 3; Meter.prototype.SetPercent has no movie caller on this instance; movie registers key 'SetEnemyHealthPercent' -> EnemyHealthMeter.SetPercent",
   ["SetPercent", "SetEnemyHealthPercent"], OBJ + " kEnemyHealthMeter component index 3",
   NONE + " (no hostile-target / enemy health state)", "no",
   "Exe has the generic names SetPercent / SetTargetPercent but not the callback key SetEnemyHealthPercent: native invokes the registered component's methods directly. 0..100"))
A(("_root.HUDMovieBaseInstance.EnemyHealthMeter.SetTargetPercent(aPercent)", "meter", "yes", "no",
   f"{T}: same component; key 'SetEnemyHealthTargetPercent' -> EnemyHealthMeter.SetTargetPercent; Meter.Update animates toward TargetPercent each onEnterFrame",
   ["SetTargetPercent", "SetEnemyHealthTargetPercent"], OBJ + " kEnemyHealthMeter", NONE, "no", ""))
A(("_root.HUDMovieBaseInstance.EnemyHealth_mc.BracketsInstance.RolloverNameInstance", "text", "yes", "no",
   f"{T}: RegisterHUDComponents arg 7; movie only sets textAutoSize='shrink' in the HUDMenu ctor",
   [], OBJ + " kRolloverNameInstance (7), kBracketsInstance (6)", NONE + " (target name)", "no",
   "Native holds the text field and the brackets clip and writes the enemy name there"))
A(("_root.HUDMovieBaseInstance.StealthMeterInstance", "visibility", "yes", "no",
   f"{T}: RegisterHUDComponents args 4,5,8,9 = StealthMeterInstance, .SneakAnimInstance, .SneakTextHolder, .SneakTextHolder.SneakTextClip.SneakTextInstance; ctor only gotoAndStop('FadedOut'); frame labels FadeIn/FadeOut/FadedOut and mode flag StealthMode in HUDMenu",
   ["$HIDDEN", "$DETECTED", "StealthMode", "FadeIn", "FadeOut"], CLSSE + "S/StealthMeter.h (StealthMeter : HUDObject holding the four GFxValues) ; " + OBJ,
   NONE + " (detection state; grep for struct Detect*/Stealth* found no component)", "no",
   "Text values $HIDDEN / $DETECTED are translation keys the native side writes into SneakTextInstance"))

# Compass
A(("_root.HUDMovieBaseInstance.SetCompassAngle(aPlayerAngle, aCompassAngle, abShowCompass)", "compass", "yes", "no",
   f"{T}: HUDMenu.prototype.SetCompassAngle: CompassRect._parent._visible=abShowCompass; CompassRect._x=Lerp(CompassZeroX,CompassThreeSixtyX,0,360,aCompassAngle); then UpdateCompassMarkers(aPlayerAngle); no movie caller; {cb('SetCompassAngle')}",
   ["SetCompassAngle"], "", HDG, "yes",
   "Both angles are degrees in 0..360. The AS uses aCompassAngle for the strip offset and aPlayerAngle as the marker centre; whether the engine passes two different values is NOT established, feeding the same heading to both is the only reading the AS supports"))
A(("_root.HUDMovieBaseInstance.CompassTargetDataA", "compass", "yes", "yes",
   f"{T}: HUDMenu ctor `CompassTargetDataA = new Array()`; SetCompassMarkers reads it with stride 4 (heading at +0, alpha at +1, marker frame at +2, scale at +3)",
   ["CompassTargetDataA"], "", NONE + " (nearest inputs: QuestObjectiveState in crates/scripting/src/quest_stages.rs:444 for quest targets, MapMarkers in crates/scripting/src/map_markers.rs:21 for script-visible map markers; no enemy / location compass list)", "yes",
   "The movie creates the empty array; native fills it then calls SetCompassMarkers. Fill requires path-level write (not in the callback table); frame values are the CompassMarker* constants below"))
A(("_root.HUDMovieBaseInstance.SetCompassMarkers()", "compass", "yes", "no",
   f"{T}: HUDMenu.prototype.SetCompassMarkers (attachMovie 'Compass Marker' into CompassRect.QuestHolder / MarkerHolder from CompassTargetDataA); no movie caller; {cb('SetCompassMarkers')}",
   ["SetCompassMarkers"], "", "see CompassTargetDataA", "yes", "No arguments: the data is the array above"))
for c, fr in (("CompassMarkerQuest", "Quest"), ("CompassMarkerQuestDoor", "QuestDoor"), ("CompassMarkerPlayerSet", "PlayerSet"), ("CompassMarkerEnemy", "Enemy"), ("CompassMarkerLocations", "LocationMarkers"), ("CompassMarkerUndiscovered", "UndiscoveredMarkers")):
    A((f"_root.HUDMovieBaseInstance.{c}", "compass", "no", "yes",
       f"{T}: HUDMenu.InitCompass sets it to the _currentframe of a temporary 'Compass Marker' clip after gotoAndStop('{fr}')",
       [c], "", "n/a (native READS it to learn the frame number)", "yes",
       "Native-read constant, not a feed: the exe string appears beside CompassTargetDataA, so native looks the frame up here when filling the array"))
A(("_root.HUDMovieBaseInstance.CompassShoutMeterHolder.Compass.DirectionRect._x", "compass", "no", "yes",
   f"{T}: set by SetCompassAngle (CompassRect = CompassShoutMeterHolder.Compass.DirectionRect, HUDMenu ctor)",
   [], "", "n/a (derived from SetCompassAngle)", "yes", "Do not write directly; the strip offset is the movie's own Lerp"))

# Modes / crosshair / text
A(("_root.HUDMovieBaseInstance.ShowElements(aMode, abShow)", "visibility", "yes", "no",
   f"{T}: HUDMenu.prototype.ShowElements pushes/pops aMode on HUDModes then sets _visible on each HudElements entry by hasOwnProperty(mode); SetModes lists mode properties All, Favor, InventoryMode, TweenMode, BookMode, DialogueMode, BarterMode, WorldMapMode, MovementDisabled, StealthMode, Swimming, HorseMode, WarHorseMode, CartMode; no movie caller; {cb('ShowElements')}",
   ["ShowElements", "Favor", "InventoryMode", "TweenMode", "BookMode", "DialogueMode", "BarterMode", "WorldMapMode", "MovementDisabled", "StealthMode", "Swimming", "HorseMode", "WarHorseMode", "CartMode", "HUDModes"], "",
   NONE + " (HUD mode stack; nearest HudControl.visible, byroredux/src/hud.rs:253, is on/off only)", "no",
   "Mode names are the 14 listed (the exe carries each as an exact line; 'All' is the default and not an exe line)"))
A(("_root.HUDMovieBaseInstance.SetCrosshairTarget(abActivate, aName, abShowButton, abTextOnly, abFavorMode, abShowCrosshair, aWeight, aCost, aFieldValue, aFieldText)", "text", "yes", "no",
   f"{T}: HUDMenu.prototype.SetCrosshairTarget sets RolloverText / RolloverInfoText / RolloverGrayBar_mc / CrosshairInstance frames; movie only calls it with (false,'') in its ctor and from SetLoadDoorInfo; {cb('SetCrosshairTarget')}",
   ["SetCrosshairTarget"], CLSSE + "H/HUDData.h (HUDData::Type kActivate / kActivateNoLabel / kCrosshairHide / kCrosshairSneak) ; " + MNU,
   "InteractionState / InteractionTarget (byroredux/src/interaction.rs:778-790) for the looked-at target; item weight / value from its base record", "no",
   "Activation prompt text plus value/weight/field line"))
A(("_root.HUDMovieBaseInstance.SetCrosshairEnabled(abFlag)", "flag", "yes", "no",
   f"{T}: HUDMenu.prototype.SetCrosshairEnabled; no movie caller; {cb('SetCrosshairEnabled')}", ["SetCrosshairEnabled"], "", "settings: crosshair visibility (debug_ui SHOW_CROSSHAIR_SETTING_ID, byroredux/src/app_frame.rs:330)", "no", ""))
A(("_root.HUDMovieBaseInstance.ValidateCrosshair()", "visibility", "yes", "no",
   f"{T}: HUDMenu.prototype.ValidateCrosshair; no movie caller; {cb('ValidateCrosshair')}", ["ValidateCrosshair"], "", "n/a (re-evaluates mode visibility)", "no", ""))
A(("_root.HUDMovieBaseInstance.SetLoadDoorInfo(abShow, aDoorName)", "text", "yes", "no",
   f"{T}: HUDMenu.prototype.SetLoadDoorInfo; no movie caller; {cb('SetLoadDoorInfo')}", ["SetLoadDoorInfo"], "", "InteractionState (door target name)", "no", ""))
A(("_root.HUDMovieBaseInstance.RefreshActivateButtonArt(astrButtonName)", "text", "yes", "no",
   f"{T}: HUDMenu.prototype.RefreshActivateButtonArt; no movie caller; NOT in the callback table (path-invoke only)", ["RefreshActivateButtonArt"], "", NONE + " (activate-button glyph name)", "no", "Passing undefined clears the glyph"))
A(("_root.HUDMovieBaseInstance.SetLocationName(aLocation)", "text", "yes", "no",
   f"{T}: HUDMenu.prototype.SetLocationName -> LocationLockBase.LocationNameBase.LocationTextBase.LocationTextInstance.SetText; no movie caller; {cb('SetLocationName')}",
   ["SetLocationName"], "", NONE + " (CurrentCellContext.cell_editor_id, byroredux/src/components/indices.rs:159, is an editor id, not the display name)", "no", "Discovery banner text"))
A(("_root.HUDMovieBaseInstance.ShowMessage(asMessage)", "text", "yes", "no",
   f"{T}: HUDMenu.prototype.ShowMessage pushes onto MessagesInstance.MessageArray; no movie caller; {cb('ShowMessage')}",
   ["ShowMessage", "MessagesInstance", "MessageArray", "ShownMessageArray"], "", "PlayerNotifications (byroredux/src/notifications.rs:14, drain())", "no", "Top-left message queue"))
A(("_root.HUDMovieBaseInstance.ShowSubtitle(astrText)", "text", "yes", "no",
   f"{T}: HUDMenu.prototype.ShowSubtitle; no movie caller; {cb('ShowSubtitle')}", ["ShowSubtitle"], CLSSE + "S/SubtitleManager.h (SubtitleInfo speaker/subtitle/targetDistance/forceDisplay)",
   "DialoguePlayback.active_lines (crates/scripting/src/dialogue.rs:239)", "no", "HideSubtitle and SetSubtitlesEnabled are the pair"))
A(("_root.HUDMovieBaseInstance.HideSubtitle()", "text", "yes", "no",
   f"{T}: HUDMenu.prototype.HideSubtitle; no movie caller; {cb('HideSubtitle')}", ["HideSubtitle"], "", "DialoguePlayback.active_lines", "no", ""))
A(("_root.HUDMovieBaseInstance.SetSubtitlesEnabled(abEnable)", "flag", "yes", "no",
   f"{T}: HUDMenu.prototype.SetSubtitlesEnabled; no movie caller; {cb('SetSubtitlesEnabled')}", ["SetSubtitlesEnabled"], "", "settings: subtitles on/off (INI bShowSubtitleSpeakerName is a sibling; no Redux setting today)", "no", ""))
A(("_root.HUDMovieBaseInstance.ShowArrowCount(aCount, abHide, aArrows)", "text", "yes", "no",
   f"{T}: HUDMenu.prototype.ShowArrowCount; no movie caller; NOT in the callback table (path-invoke only)", ["ShowArrowCount"], CLSSE + "H/HUDData.h (HUDData::Type kShowArrowCount = 7)",
   "Inventory component (crates/core/src/ecs/components/inventory.rs) equipped ammo count; no dedicated HUD state", "no", "aArrows is the localized label"))
A(("_root.HUDMovieBaseInstance.ShowTutorialHintText(astrHint, abShow)", "text", "yes", "no",
   f"{T}: HUDMenu.prototype.ShowTutorialHintText; no movie caller; {cb('ShowTutorialHintText')}", ["ShowTutorialHintText"], "", NONE, "no", ""))
A(("_root.HUDMovieBaseInstance.QuestUpdateBaseInstance.ShowNotification(aNotificationText, aStatus, aSoundID, aObjectiveCount, aNotificationType, aLevel, aStartPercent, aEndPercent, aDragonText)", "text", "yes", "no",
   f"{T}: QuestNotification.prototype.ShowNotification (DoInitAction sprite 865); registered as key 'ShowNotification' with scope QuestUpdateBaseInstance; QuestUpdateBaseInstance is RegisterHUDComponents arg 2; no movie caller",
   ["ShowNotification", "CanShowNotification", "ObjectivesA", "QuestNotification.AnimLetter.QuestName", "QuestNotification.ShoutLetter.QuestName"],
   CLSSE + "H/HUDNotifications.h (HUDNotifications : HUDObject, root comment 'HUDMovieBaseInstance.QuestUpdateBaseInstance', objectives comment 'QuestNotification.ObjectivesA') ; " + CLSSE + "H/HUDData.h (kQuestStarted..kWordOfPower2, kSkillIncrease)",
   "QuestObjectiveState / QuestObjectiveEvent (crates/scripting/src/quest_stages.rs:444,477); level/skill XP: ActorValues", "no",
   "Type is the aNotificationType (QuestNotification.QUEST_UPDATE / SHOUT_UPDATE constants); aStartPercent/aEndPercent are 0..1 level-meter fractions (x100 in the movie)"))
A(("_root.HUDMovieBaseInstance.QuestUpdateBaseInstance.CanShowNotification()", "flag", "no", "no",
   f"{T}: QuestNotification.prototype.CanShowNotification returns ShowNotifications && AnimationCount==0; no movie caller",
   ["CanShowNotification"], CLSSE + "H/HUDNotifications.h (queue + time members: native paces the queue)", "n/a (native READS the return value to pace its queue)", "no",
   "A query the native side makes, not a feed; a Ruffle adapter would need a return value path"))
A(("_root.HUDMovieBaseInstance.QuestUpdateBaseInstance.ObjectivesA", "text", "yes", "yes",
   f"{T}: QuestNotification ctor `ObjectivesA = new Array()`; ShowObjectives/DuplicateObjective consume it (shift) as groups of strings",
   ["ObjectivesA"], CLSSE + "H/HUDNotifications.h", "QuestObjectiveState (crates/scripting/src/quest_stages.rs:444)", "no",
   "Native pushes the objective strings here before ShowNotification"))
A(("_root.HUDMovieBaseInstance.FloatingQuestMarkerInstance", "other", "unclear", "no",
   f"{T}: placed on the HUD timeline (char 754) and listed in HudElements; the movie scripts nothing into it",
   ["FloatingQuestMarkerInstance", "quest target_HUD", "target%i"], "", NONE, "no",
   "Exe strings 'FloatingQuestMarkerInstance', 'quest target_HUD' and 'target%i' suggest native attaches numbered marker clips; the attach call is not visible in strings, so unclear"))
for m in ("TopLeftRefX", "TopLeftRefY", "BottomRightRefX", "BottomRightRefY"):
    A((f"_root.HUDMovieBaseInstance.{m}", "other", "no", "yes",
       f"{T}: HUDMenu.InitExtensions sets it from {m[:-1]}Instance.LocationRefInstance via localToGlobal",
       [m], "", "n/a (native READS the safe-rect anchor)", "no", "Layout anchor the engine reads back; not a feed"))
A(("_root.gfx.io.GameDelegate.receiveCall(methodName, ...args) via ExternalInterface callback 'call'", "other", "unclear", "yes",
   f"{T}: GameDelegate.initialize registers ExternalInterface callbacks 'call'->receiveCall and 'respond'->receiveResponse; receiveCall dispatches callBackHash[methodName] with arguments.slice(1); HUDMenu.InitExtensions fills callBackHash with 26 keys",
   [], "", "n/a (transport)", "no",
   "Alternative transport: 21 of the 26 registered keys also appear as exact lines in SkyrimSE.exe strings, 5 do not (SetCompassTemperature, SetEnemyHealthPercent, SetEnemyHealthTargetPercent, StartMagickaMeterBlinking, StartStaminaMeterBlinking), so the exe strings point at invocation by method name on the instance, not through this table. That a host CAN drive the HUD through callback 'call' follows from the bytecode; it is not evidence about what the original engine does"))

emit("hud-feeds-skyrim-se.tsv", sky_exe, "SkyrimSE.exe", sky)

# ───────────────────────── Fallout 4 ─────────────────────────
F = "hud_feed_paths fo4 dump (interface\\hudmenu.swf in Fallout4 - Interface.ba2; document class HUDMenu = SymbolClass char 0)"
FEXE = "Fallout4.exe"
VITF = "ActorValues of PlayerEntity via PlayerVitals::resolved(editor id) (byroredux/src/scaleform_hud.rs bar_fractions)"
HDGF = HDG
fo = []
B = fo.append
HB = "https://scrivener07.github.io/BGS-Handbook/wiki_ui_menus/fo4/interface/reference/menus/hud"

def meterbar(path_prefix, cls_ctx, notes, canon, fed="yes", comp="no"):
    pass

B(("HUDMenu (document root)", "other", "yes", "no",
   f"{F}: HUDMenu extends Shared.IMenu declares the nine group clips + BGSCodeObj; only methods onSetSafeRect/onCodeObjCreate/onCodeObjDestruction; no data members or setters for any meter",
   ["HUDMenu"], HB + " (HUDMenu.as members) ; " + F4CF + "HUDMenu.as",
   "n/a (root)", "no",
   "The document is a layout shell: every value lives on a child component. F4SE-side root prefix for these paths is not established here"))

PATHS = [
 ("LeftMeters_mc.HPMeter_mc", "health"),
 ("LeftMeters_mc.RadsMeter_mc", "rads"),
 ("RightMeters_mc.ActionPointMeter_mc", "ap"),
 ("TopCenterGroup_mc.EnemyHealthMeter_mc", "enemy"),
 ("BottomCenterGroup_mc.CritMeter_mc", "crit"),
]
EXE_LIST = "the exe path-list cluster (lines 'LeftMeters_mc.HPMeter_mc', 'RightMeters_mc.ActionPointMeter_mc', 'TopCenterGroup_mc.EnemyHealthMeter_mc', 'BottomCenterGroup_mc.CompassWidget_mc', ...)"
B(("LeftMeters_mc.HPMeter_mc.MeterBar_mc.Percent", "meter", "yes", "no",
   f"{F}: HPMeter_41 (HUDMenu_fla) children MeterBar_mc/Optional_mc/RadsBar_mc : MeterBar : MeterBarWidget; MeterBarWidget.set Percent -> _percent, redrawUIComponent does scaleX=Percent; the only movie writers are the IDE initialiser __setProp_MeterBar_mc_HPMeter_MeterBar_mc_0 (Percent=0, BarAlpha=1) -- `fo4 refs Percent`",
   ["LeftMeters_mc.HPMeter_mc", "Percent", "MeterBar_mc"], F4CF + "MeterBarWidget.as, " + F4CF + "HUDMenu_fla/HPMeter_41.as (reconstructed source of the same SWF)",
   VITF + " -> editor id 'Health' (current / max)", "no",
   "Unit is 0..1 (scaleX = Percent), unlike Skyrim's 0..100. Engine-fed is INFERRED: nothing in the movie writes Percent after init and the exe lists the HPMeter_mc path and the identifier 'Percent'"))
B(("LeftMeters_mc.HPMeter_mc.Optional_mc.Percent", "meter", "yes", "no",
   f"{F}: HPMeter_41.Optional_mc : MeterBar (BarAlpha=0.5 in __setProp_Optional_mc_HPMeter_Optional_mc_0); Percent never written after init",
   ["Percent", "Optional_mc"], F4CF + "HUDMenu_fla/HPMeter_41.as",
   VITF + " (health damage 'ghost' / limb-damage overlay: exact semantic not established)", "no",
   "Second bar behind the main one; which quantity native writes (damage trail vs. healing preview) is not established -> treat as unclear until observed"))
B(("LeftMeters_mc.HPMeter_mc.RadsBar_mc.Percent", "meter", "yes", "no",
   f"{F}: HPMeter_41.RadsBar_mc : MeterBar; Percent never written after init",
   ["Percent", "RadsBar_mc", "RadsMeter_mc"], F4CF + "HUDMenu_fla/HPMeter_41.as",
   VITF + " -> radiation (AVIF editor id for rads; PlayerVitals.radiation carries the scale for FO3/FNV only)", "no",
   "Rads overlay on the health bar; 0..1 of max health"))
B(("LeftMeters_mc.HPMeter_mc.DisplayText_tf", "text", "yes", "no",
   f"{F}: HPMeter_41.DisplayText_tf declared; `fo4 refs DisplayText_tf` returns no get/set/call in any method body",
   ["DisplayText_tf"], "", "ActorValues Health (rounded) when the INI shows numbers", "no", "Text field the movie never touches; native writes or hides it"))
B(("LeftMeters_mc.RadsMeter_mc", "meter", "unclear", "no",
   f"{F}: LeftMeters stage child char 571 (no class); exe path-list line 'LeftMeters_mc.RadsMeter_mc'",
   ["LeftMeters_mc.RadsMeter_mc"], "", "ActorValues radiation", "no", "Instance class not resolved by this tool (stage symbol without ABC class)"))
B(("LeftMeters_mc.LocationText_mc.RegionText", "text", "yes", "no",
   f"{F}: LeftMeters child LocationText_mc (char 573); movie code touches neither; exe path-list line 'LeftMeters_mc.LocationText_mc.RegionText'",
   ["LeftMeters_mc.LocationText_mc.RegionText"], "", NONE + " (CurrentCellContext holds the cell editor id only)", "no", "Location discovery text"))
B(("RightMeters_mc.ActionPointMeter_mc.MeterBar_mc.Percent", "meter", "yes", "no",
   f"{F}: ActionPointMeter (extends MovieClip) MeterBar_mc/Optional_mc; Percent written only by __setProp_MeterBar_mc_ActionPointMeter_MeterBar_mc_0 (=0) -- `fo4 refs Percent`; ActionPointMeter.AddSegments has no movie caller",
   ["RightMeters_mc.ActionPointMeter_mc", "Percent", "AddSegments"], F4CF + "ActionPointMeter.as",
   VITF + " -> editor id 'ActionPoints'", "no",
   "0..1 as for HP. Engine-fed inferred, same basis. Child is typed MovieClip but is a MeterBar instance on stage"))
B(("RightMeters_mc.ActionPointMeter_mc.AddSegments(param1:Array)", "meter", "yes", "no",
   f"{F}: ActionPointMeter.AddSegments(Array of fractions) builds ActionPointBarSegment clips of width fraction*TotalBarWidth; no caller in the movie (`fo4 refs AddSegments` empty)",
   ["AddSegments", "UIActionPointsLow"], F4CF + "ActionPointMeter.as",
   NONE + " (VATS action-point segment costs)", "no",
   "Segment fractions of the AP bar for queued VATS actions; exe also carries UIActionPointsLow"))
B(("RightMeters_mc.ActionPointMeter_mc.DisplayText_tf", "text", "yes", "no",
   f"{F}: ActionPointMeter.DisplayText_tf declared; no get/set/call anywhere in the movie", ["DisplayText_tf"], "", "ActorValues ActionPoints", "no", ""))
B(("TopCenterGroup_mc.EnemyHealthMeter_mc.MeterBar_mc.Percent", "meter", "yes", "no",
   f"{F}: EnemyHealthMeter_mc (char 637) children Optional_mc/MeterBar_mc/Bracket_mc/DisplayText_tf/SkullIcon_mc/LegendaryIcon_mc; Percent never written after init; exe path-list line 'TopCenterGroup_mc.EnemyHealthMeter_mc'",
   ["TopCenterGroup_mc.EnemyHealthMeter_mc", "SkullIcon_mc", "LegendaryIcon_mc", "DisplayText_tf"], "",
   NONE + " (no hostile-target / enemy health state)", "no", "Enemy name in DisplayText_tf, skull and legendary icons toggled natively (movie never touches them)"))
B(("BottomCenterGroup_mc.CritMeter_mc.CritMeterStars_mc.SetCritStars", "meter", "yes", "no",
   f"{F}: CritMeterStarHolder.SetCritStars (sets _numStarsFilled / _numStarsShown, redraw reads them); no movie caller (`fo4 refs SetCritStars` empty); CritMeter_34 children MeterBar_mc : CritMeterBar",
   ["SetCritStars", "CritMeterStars_mc", "BottomCenterGroup_mc.CritMeter_mc"], F4CF + "CritMeterStarHolder.as",
   NONE + " (VATS critical meter)", "no", "Critical-hit meter, VATS only; stars filled/shown counts"))
B(("BottomCenterGroup_mc.CritMeter_mc.MeterBar_mc.Percent", "meter", "yes", "no",
   f"{F}: CritMeter_34.MeterBar_mc : CritMeterBar : MeterBarWidget; Percent never written after init", ["Percent", "CritMeterStars_mc"], "", NONE + " (VATS critical meter)", "no", "0..1"))
B(("HUDNotificationsGroup_mc.XPMeter_mc.LevelUPBar.Percent", "meter", "yes", "no",
   f"{F}: XPMeter_117.LevelUPBar : MeterBar; Percent never written after init; children xptext, NumberText, LeveUpTextClip (LeftToRightTextAnim.AnimateText)",
   ["HUDNotificationsGroup_mc.XPMeter_mc", "LevelUPBar", "xptext", "NumberText", "LeveUpTextClip", "AnimateText"], "",
   "ActorValues XP / level progress (FO4 profile not verified)", "no", "XP gain bar and 'You've reached level N' text"))

# Stealth
B(("TopCenterGroup_mc.StealthMeter_mc.SetStealthMeter", "visibility", "yes", "no",
   f"{F}: StealthMeter.SetStealthMeter (writes LastPercent, StealthTextInstance.SetText); no movie caller (`fo4 refs SetStealthMeter` empty); exe path-list line 'TopCenterGroup_mc.StealthMeter_mc'",
   ["SetStealthMeter", "TopCenterGroup_mc.StealthMeter_mc"], "", NONE + " (detection state)", "no", "Sneak state text (hidden / caution / danger) and bar"))

# Compass
B(("BottomCenterGroup_mc.CompassWidget_mc", "compass", "yes", "no",
   f"{F}: HUDCompassWidget extends BSUIComponent with ONLY stage children CompassBar_mc, QuestMask_mc, QuestMarkerHolder_mc, OtherMask_mc, OtherMarkerHolder_mc, CompassBGHolder_mc; no methods, no logic; exe path-list line 'BottomCenterGroup_mc.CompassWidget_mc' plus bShowCompass:Interface, fCompMarkerY / fCompMarkerHeight, $Compass_North/East/South/West",
   ["BottomCenterGroup_mc.CompassWidget_mc", "CompassBar_mc", "QuestMarkerHolder_mc", "OtherMarkerHolder_mc", "QuestMask_mc", "OtherMask_mc", "CompassBGHolder_mc", "$Compass_North", "$Compass_East", "$Compass_South", "$Compass_West", "bShowCompass:Interface"], F4CF + "HUDCompassWidget.as (empty body) ; " + HB,
   HDGF, "yes",
   "The whole compass is native-driven: heading, scrolling and marker placement have NO bytecode in the movie. Which child property native writes to scroll (CompassBar_mc.x vs. a frame) is NOT established by this tool or by any source found -> a driver must be built from observing the engine or from the visual structure, not from this table"))
B(("BottomCenterGroup_mc.CompassWidget_mc.QuestMarkerHolder_mc / OtherMarkerHolder_mc (marker clips)", "compass", "yes", "no",
   f"{F}: holders are empty containers (char 589) masked by QuestMask_mc / OtherMask_mc; marker clip classes with frames: CompassMarkerWidget (7 frames), HUDMenu_fla.QuestCompassMarkers_136 / QuestDoorCompassMarkers_140 / PlayerSetCompassMarker_143 / CompassMarker_Enemy_148 / ReconScopeMarker_146 each with Distance_tf + MarkerShape_mc; exe carries marker kinds QuestDoor, PlayerSet, EnemyTargeted, Follower, Recon, BelowPlayer, LevelWithPlayer, AbovePlayer, Undiscovered, Targeted and ~90 *Marker location classes",
   ["QuestMarkerHolder_mc", "OtherMarkerHolder_mc", "Distance_tf", "MarkerShape_mc", "Direction_tf", "MarkerIcon_mc", "EnemyTargeted", "Follower", "Recon", "BelowPlayer", "LevelWithPlayer", "AbovePlayer", "Undiscovered", "Targeted"], "",
   NONE + " (nearest: QuestObjectiveState quest_stages.rs:444; MapMarkers map_markers.rs:21; no enemy / follower list)", "yes",
   "Native instantiates marker clips by class/frame and positions them; each carries a distance text. Per-marker property set not established"))
B(("PowerArmorHUDMenu (interface\\powerarmorhudmenu.swf).CompassWidget_mc", "compass", "yes", "no",
   "hud_feed_paths fo4 dump interface\\powerarmorhudmenu.swf: document class PowerArmorHUDMenu (char 0) has root.CompassWidget_mc = HUDCompassWidget (char 292) with CompassBar_mc / CompassBGHolder_mc and the same marker class set",
   ["PowerArmorHUDMenu", "CompassWidget_mc"], F4CF.replace("HUDMenu/", "PowerArmorHUDMenu/") + "HUDCompassWidget.as",
   HDGF, "yes", "Second movie with its own compass, used while in power armor; same unknowns as the standard compass. Power armor also draws its own meters (not tabulated here)"))

# Crosshair / rollover
B(("CenterGroup_mc.HUDCrosshair_mc.CrosshairBase_mc.requestedState", "other", "yes", "no",
   f"{F}: CrosshairBase.set requestedState / requestedRadius only store _requestedState/_requestedRadius; redrawUIComponent consumes them (STATE_NONE/DOT/STANDARD/ACTIVATE/COMMAND constants); no movie writer (`fo4 refs requestedState`)",
   ["requestedState", "requestedRadius", "CrosshairBase_mc", "CenterGroup_mc.HUDCrosshair_mc"], F4CF + "CrosshairBase.as",
   "InteractionState / InteractionTarget (byroredux/src/interaction.rs:778-790) -> state; weapon spread for radius (none today)", "no",
   "State strings from the CrosshairBase constants; radius in pixels"))
B(("CenterGroup_mc.RolloverWidget_mc", "text", "yes", "no",
   f"{F}: RolloverWidget declares RolloverName_tf, LegendaryIcon_mc, TaggedForSearchIcon_mc, ButtonHintBar_mc, ActivateButton / PlayHolotapeButton (BSButtonHintData); movie calls only PopulateButtonBar/AdjustRolloverPositions internally; exe path-list line 'CenterGroup_mc.RolloverWidget_mc'",
   ["CenterGroup_mc.RolloverWidget_mc", "RolloverName_tf", "LegendaryIcon_mc", "ActivateButton", "PlayHolotapeButton"], "",
   "InteractionState / InteractionTarget (byroredux/src/interaction.rs:778-790)", "no", "Activation prompt text, icons and button hints"))
B(("CenterGroup_mc.QuickContainerWidget_mc.ItemDataA", "other", "yes", "no",
   f"{F}: QuickContainerWidget.ItemDataA (Vector of QuickContainerItemData) consumed by UpdateList / UpdateComponentPositions; setters containerName, bracketsVisible; no movie writer", ["ItemDataA", "containerName", "bracketsVisible", "CenterGroup_mc.QuickContainerWidget_mc"], "", NONE + " (quick-container list)", "no", ""))

# Text / notifications
B(("BottomCenterGroup_mc.SubtitleText_mc (SpeakerName_tf, SubtitleText_tf)", "text", "yes", "no",
   f"{F}: Subtitles extends BSUIComponent: SpeakerName_tf, SubtitleText_tf, setter VerticalAutoSize; no movie code writes either field; exe path-list line 'BottomCenterGroup_mc.SubtitleText_mc'",
   ["BottomCenterGroup_mc.SubtitleText_mc", "SubtitleText_tf", "SpeakerName_tf"], "", "DialoguePlayback.active_lines (crates/scripting/src/dialogue.rs:239)", "no", "HDT_SHOW_SUBTITLE / HDT_HIDE_SUBTITLE appear in the exe HUD data-type list"))
B(("HUDNotificationsGroup_mc.Messages_mc.MessageArray", "text", "yes", "no",
   f"{F}: Messages.MessageArray (Vector of HUDMessageItemData) drained by Messages.Update; created empty in the ctor; no movie writer", ["MessageArray", "HUDNotificationsGroup_mc.Messages_mc", "HUDMessageItemData"], "", "PlayerNotifications (byroredux/src/notifications.rs:14)", "no", ""))
B(("HUDNotificationsGroup_mc.ObjectiveUpdates_mc.ObjectiveDataV", "text", "yes", "no",
   f"{F}: HUDObjectiveUpdates.ObjectiveDataV (Vector of HUDObjectiveItemData) read by Update/AddObjective*; ctor creates it; no movie writer", ["ObjectiveDataV", "HUDNotificationsGroup_mc.ObjectiveUpdates_mc", "HUDObjectiveItemData", "ClearObjectives"], "", "QuestObjectiveState (crates/scripting/src/quest_stages.rs:444)", "no", "HDT_OBJECTIVE_ADDED / COMPLETED / FAILED in the exe HUD data-type list"))
B(("HUDNotificationsGroup_mc.QuestUpdates_mc (QuestName_tf, UpdateType_tf)", "text", "yes", "no",
   f"{F}: QuestUpdates declares two TextFields only; movie never touches them; exe path-list line 'HUDNotificationsGroup_mc.QuestUpdates_mc'", ["HUDNotificationsGroup_mc.QuestUpdates_mc", "QuestName_tf", "UpdateType_tf"], "", "QuestObjectiveEvent (crates/scripting/src/quest_stages.rs:477)", "no", "HDT_QUEST_ADDED / COMPLETED / FAILED"))
B(("HUDNotificationsGroup_mc.TutorialText_mc.SetText", "text", "yes", "no",
   f"{F}: TutorialText.SetText + numberOfHeads setter; the only caller is the IDE initialiser setting numberOfHeads; SetText has no movie caller", ["HUDNotificationsGroup_mc.TutorialText_mc"], "", NONE, "no", ""))
B(("RightMeters_mc.PowerArmorLowBatteryWarning_mc.WarningTextHolder_mc.warningText", "text", "yes", "no",
   f"{F}: PAWarningText.set warningText calls PowerArmorLowBatteryWarning_tf.SetText via BGSCodeObj; HUDMenu.onCodeObjCreate hands the code object over", ["warningText", "PowerArmorLowBatteryWarning_mc"], HB, NONE + " (power armor battery)", "no", "The only BGSCodeObj reference in the movie besides HUDMenu's own field (`fo4 refs BGSCodeObj`)"))
B(("RightMeters_mc.FatigueWarning_mc", "visibility", "yes", "no",
   f"{F}: FatigueWarning_mc stage child (char 615); movie scripts nothing; exe carries FatigueWarning_tf / FatigueHead_mc and uFatigueWarningDisplayDurationMS:Interface", ["RightMeters_mc.FatigueWarning_mc", "FatigueWarning_tf", "FatigueHead_mc"], "", NONE, "no", "Sleep/fatigue warning"))
B(("RightMeters_mc.AmmoCount_mc (ClipCount_tf, ReserveCount_tf)", "text", "yes", "no",
   f"{F}: AmmoCount_mc stage child (char 619) has no ABC class; exe carries ClipCount_tf, ReserveCount_tf, AvailableCount_tf, TypeIcon_mc", ["RightMeters_mc.AmmoCount_mc", "ClipCount_tf", "ReserveCount_tf"], "", "Inventory component (crates/core/src/ecs/components/inventory.rs) equipped ammo", "no", "Clip and reserve ammo counts"))
B(("RightMeters_mc.ExplosiveAmmoCount_mc (AvailableCount_tf, TypeIcon_mc)", "text", "yes", "no",
   f"{F}: ExplosiveAmmoCount extends BSUIComponent, no members; child fields AvailableCount_tf / TypeIcon_mc untouched by movie code", ["RightMeters_mc.ExplosiveAmmoCount_mc", "AvailableCount_tf", "TypeIcon_mc"], "", "Inventory component (grenade / mine counts)", "no", ""))
B(("RightMeters_mc.HUDActiveEffectsWidget_mc", "other", "yes", "no",
   f"{F}: HUDActiveEffectsWidget(bInPowerArmorMode, MAX_NUM_CLIPS, CLIP_HEIGHT, CLIP_SPACER, ClipHolderInternal, CLIP_FLASH_CLASS) + HUDActiveEffectClip.IconFrame; no movie writer of bInPowerArmorMode / IconFrame", ["RightMeters_mc.HUDActiveEffectsWidget_mc", "IconFrame", "bInPowerArmorMode", "ClipHolderInternal", "CLIP_FLASH_CLASS", "MAX_NUM_CLIPS"], "", NONE + " (active effects list; nearest: active magic effect components in crates/core)", "no", "Chem / effect icons"))
B(("RightMeters_mc.FlashLightWidget_mc", "flag", "unclear", "no",
   f"{F}: FlashLightWidget extends BSUIComponent with no members; exe path-list line 'RightMeters_mc.FlashLightWidget_mc'", ["RightMeters_mc.FlashLightWidget_mc"], "", NONE, "no", "Which property native writes is not recoverable from an empty class"))
B(("TopRightGroup_mc.VaultBoyCondition_mc", "other", "yes", "no",
   f"{F}: HUDConditionBoyWidget.ConditionBoy_mc : ConditionBoy with setters BodyFlags / HeadFlags (redraw loads PipboyLoader clips); no movie writer", ["TopRightGroup_mc.VaultBoyCondition_mc", "BodyFlags", "HeadFlags", "ConditionBoy_mc"], "", NONE + " (limb condition flags)", "no", "Limb injury indicator"))
B(("CenterGroup_mc.HitIndicator_mc / DirectionalHitIndicatorBase_mc / ExplosiveIndicatorBase_mc", "other", "yes", "no",
   f"{F}: HitIndicator_101.HitHolder, DirectionalHitIndicator.Arc_mc, ExplosiveIndicator.Arrow_mc + CarIcon_mc/GrenadeIcon_mc/MineIcon_mc; no movie logic; exe carries the CenterGroup_mc path lines and uDirectionalHitIndicatorLifespan", ["CenterGroup_mc.HitIndicator_mc", "CenterGroup_mc.DirectionalHitIndicatorBase_mc", "CenterGroup_mc.ExplosiveIndicatorBase_mc", "Arc_mc", "Arrow_mc"], "", NONE + " (damage direction events)", "no", "Directional damage and explosive-proximity indicators"))
B(("HUDMenu.BGSCodeObj", "other", "yes", "no",
   f"{F}: HUDMenu ctor creates BGSCodeObj = new Object(); onCodeObjCreate / onCodeObjDestruction are the lifecycle hooks the adapter already forwards", ["BGSCodeObj", "onAcquiredByNativeCode"], HB, "n/a (host contract, GUI-HOST-001)", "no",
   "Exe also carries onAcquiredByNativeCode: BSUIComponent exposes _bAcquiredByNativeCode / onAcquiredByNativeCode, the hook native calls after it adopts a component"))

emit("hud-feeds-fallout4.tsv", fo4_exe, FEXE, fo)
