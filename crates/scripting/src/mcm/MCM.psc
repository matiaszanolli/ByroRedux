ScriptName MCM Hidden
{The engine's MCM (LMC-UI-002 slice 4, docs/engine/mcm-host.md §5.1): MCM Helper's global API, written
from its published MCM.psc (v1.6.2). It shadows MCM Helper's script; the engine serves MCM Helper, so
a mod probing for it finds it.}

Bool Function IsInstalled() Global
	Return True
EndFunction

; MCM Helper v1.6.2's version code.
Int Function GetVersionCode() Global
	Return McmHelper.VersionCode()
EndFunction

Int Function GetModSettingInt(String a_modName, String a_settingName) Global
	Return McmHelper.GetModSettingInt(a_modName, a_settingName)
EndFunction
Bool Function GetModSettingBool(String a_modName, String a_settingName) Global
	Return McmHelper.GetModSettingBool(a_modName, a_settingName)
EndFunction
Float Function GetModSettingFloat(String a_modName, String a_settingName) Global
	Return McmHelper.GetModSettingFloat(a_modName, a_settingName)
EndFunction
String Function GetModSettingString(String a_modName, String a_settingName) Global
	Return McmHelper.GetModSettingString(a_modName, a_settingName)
EndFunction

Function SetModSettingInt(String a_modName, String a_settingName, Int a_value) Global
	McmHelper.SetModSettingInt(a_modName, a_settingName, a_value)
EndFunction
Function SetModSettingBool(String a_modName, String a_settingName, Bool a_value) Global
	McmHelper.SetModSettingBool(a_modName, a_settingName, a_value)
EndFunction
Function SetModSettingFloat(String a_modName, String a_settingName, Float a_value) Global
	McmHelper.SetModSettingFloat(a_modName, a_settingName, a_value)
EndFunction
Function SetModSettingString(String a_modName, String a_settingName, String a_value) Global
	McmHelper.SetModSettingString(a_modName, a_settingName, a_value)
EndFunction
