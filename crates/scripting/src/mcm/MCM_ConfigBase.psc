ScriptName MCM_ConfigBase extends SKI_ConfigBase
{The engine's MCM_ConfigBase (LMC-UI-002 slice 4, docs/engine/mcm-host.md §5.1): MCM Helper's config
base, written from its published scripts (MCM_ConfigBase.psc, v1.6.2). It shadows MCM Helper's script.
The page comes from the mod's MCM/Config/<mod>/config.json: SkyUI's page and option events are
answered by the static class McmHelper, as MCM Helper answers them with its plugin's natives.}

; Raised when a control changes its setting, with the control's id.
Event OnSettingChange(String a_ID)
EndEvent

; Raised after each page reset, including the front page ("").
Event OnPageSelect(String a_page)
EndEvent

; Redraw the open page's values, after the script changed them in OnSettingChange.
Function RefreshMenu()
	McmHelper.RefreshMenu()
EndFunction

; Replace a menu control's options while its page is open.
Function SetMenuOptions(String a_ID, String[] a_options, String[] a_shortNames = None)
	McmHelper.SetMenuOptions(a_ID, a_options, a_shortNames)
EndFunction

; ---- this mod's settings ("" names the config quest's own mod) ----
Int Function GetModSettingInt(String a_settingName)
	Return McmHelper.GetModSettingInt("", a_settingName)
EndFunction
Bool Function GetModSettingBool(String a_settingName)
	Return McmHelper.GetModSettingBool("", a_settingName)
EndFunction
Float Function GetModSettingFloat(String a_settingName)
	Return McmHelper.GetModSettingFloat("", a_settingName)
EndFunction
String Function GetModSettingString(String a_settingName)
	Return McmHelper.GetModSettingString("", a_settingName)
EndFunction
Function SetModSettingInt(String a_settingName, Int a_value)
	McmHelper.SetModSettingInt("", a_settingName, a_value)
EndFunction
Function SetModSettingBool(String a_settingName, Bool a_value)
	McmHelper.SetModSettingBool("", a_settingName, a_value)
EndFunction
Function SetModSettingFloat(String a_settingName, Float a_value)
	McmHelper.SetModSettingFloat("", a_settingName, a_value)
EndFunction
Function SetModSettingString(String a_settingName, String a_value)
	McmHelper.SetModSettingString("", a_settingName, a_value)
EndFunction

; MCM Helper's plugin names the menu and its pages from the config as SkyUI registers it.
Event OnGameReload()
	String name = McmHelper.ConfigName()
	If name != ""
		ModName = name
		Pages = McmHelper.ConfigPages()
	EndIf
	parent.OnGameReload()
EndEvent

; ---- SkyUI's events, answered from the config ----
Event OnPageReset(String a_page)
	McmHelper.PageReset(a_page)
	OnPageSelect(a_page)
EndEvent
Event OnOptionHighlight(Int a_option)
	McmHelper.Highlight(a_option)
EndEvent
Event OnOptionSelect(Int a_option)
	_Changed(McmHelper.Select(a_option))
EndEvent
Event OnOptionDefault(Int a_option)
	_Changed(McmHelper.Default(a_option))
EndEvent
Event OnOptionSliderOpen(Int a_option)
	McmHelper.OpenDialog(a_option)
EndEvent
Event OnOptionSliderAccept(Int a_option, Float a_value)
	_Changed(McmHelper.AcceptNumber(a_option, a_value))
EndEvent
Event OnOptionMenuOpen(Int a_option)
	McmHelper.OpenDialog(a_option)
EndEvent
Event OnOptionMenuAccept(Int a_option, Int a_index)
	_Changed(McmHelper.AcceptIndex(a_option, a_index))
EndEvent
Event OnOptionColorOpen(Int a_option)
	McmHelper.OpenDialog(a_option)
EndEvent
Event OnOptionColorAccept(Int a_option, Int a_color)
	_Changed(McmHelper.AcceptNumber(a_option, a_color))
EndEvent
Event OnOptionKeyMapChange(Int a_option, Int a_keyCode, String a_conflictControl, String a_conflictName)
	_Changed(McmHelper.AcceptNumber(a_option, a_keyCode))
EndEvent
Event OnOptionInputOpen(Int a_option)
	McmHelper.OpenDialog(a_option)
EndEvent
Event OnOptionInputAccept(Int a_option, String a_input)
	_Changed(McmHelper.AcceptText(a_option, a_input))
EndEvent

; A control whose setting changed reports its id (MCM Helper skips an empty one).
Function _Changed(String a_id)
	If a_id != ""
		OnSettingChange(a_id)
	EndIf
EndFunction
