ScriptName SKI_ConfigBase extends Quest
{The engine's SKI_ConfigBase (LMC-UI-002, docs/engine/mcm-host.md): the SkyUI Mod Configuration Menu
contract a config script extends, written from the SkyUI wiki's MCM pages. It shadows SkyUI's own
script; its option calls write the engine's McmHost.}

; SkyUI's published values (SKI_ConfigBase.psc of the SkyUI SDK).
Int Property OPTION_FLAG_NONE = 0 AutoReadOnly
Int Property OPTION_FLAG_DISABLED = 1 AutoReadOnly
Int Property OPTION_FLAG_HIDDEN = 2 AutoReadOnly
Int Property OPTION_FLAG_WITH_UNMAP = 4 AutoReadOnly
Int Property LEFT_TO_RIGHT = 1 AutoReadOnly
Int Property TOP_TO_BOTTOM = 2 AutoReadOnly

String Property ModName Auto
String[] Property Pages Auto
String Property CurrentPage Auto Hidden
; The version the save recorded (0 until the first OnGameReload).
Int Property CurrentVersion Auto Hidden

Bool _mcmInitialized

Int Function GetVersion()
	Return 1
EndFunction

Event OnInit()
	OnGameReload()
EndEvent

; Raised once per game load (and by OnInit); a script overriding it calls parent.OnGameReload().
Event OnGameReload()
	If !_mcmInitialized
		_mcmInitialized = True
		OnConfigInit()
	EndIf
	Int version = GetVersion()
	If version != CurrentVersion
		OnVersionUpdate(version)
		CurrentVersion = version
	EndIf
	McmHost.Register(ModName, Pages)
	OnConfigRegister()
EndEvent

Event OnConfigInit()
EndEvent
Event OnConfigRegister()
EndEvent
Event OnVersionUpdate(Int a_version)
EndEvent
Event OnConfigOpen()
EndEvent
Event OnConfigClose()
EndEvent
Event OnPageReset(String a_page)
EndEvent
Event OnOptionHighlight(Int a_option)
EndEvent
Event OnOptionDefault(Int a_option)
EndEvent
Event OnOptionSelect(Int a_option)
EndEvent
Event OnOptionSliderOpen(Int a_option)
EndEvent
Event OnOptionSliderAccept(Int a_option, Float a_value)
EndEvent
Event OnOptionMenuOpen(Int a_option)
EndEvent
Event OnOptionMenuAccept(Int a_option, Int a_index)
EndEvent
Event OnOptionColorOpen(Int a_option)
EndEvent
Event OnOptionColorAccept(Int a_option, Int a_color)
EndEvent
Event OnOptionKeyMapChange(Int a_option, Int a_keyCode, String a_conflictControl, String a_conflictName)
EndEvent
Event OnOptionInputOpen(Int a_option)
EndEvent
Event OnOptionInputAccept(Int a_option, String a_input)
EndEvent

Event OnHighlightST()
EndEvent
Event OnDefaultST()
EndEvent
Event OnSelectST()
EndEvent
Event OnSliderOpenST()
EndEvent
Event OnSliderAcceptST(Float a_value)
EndEvent
Event OnMenuOpenST()
EndEvent
Event OnMenuAcceptST(Int a_index)
EndEvent
Event OnColorOpenST()
EndEvent
Event OnColorAcceptST(Int a_color)
EndEvent
Event OnKeyMapChangeST(Int a_keyCode, String a_conflictControl, String a_conflictName)
EndEvent
Event OnInputOpenST()
EndEvent
Event OnInputAcceptST(String a_input)
EndEvent

; A control the mod binds to `a_keyCode`, for key-map conflicts; "" when none.
String Function GetCustomControl(Int a_keyCode)
	Return ""
EndFunction

; ---- page layout ----

Function SetCursorPosition(Int a_position)
	McmHost.SetCursor(a_position)
EndFunction

Function SetCursorFillMode(Int a_fillMode)
	McmHost.SetFillMode(a_fillMode)
EndFunction

Function SetTitleText(String a_text)
	McmHost.SetTitleText(a_text)
EndFunction

Function SetInfoText(String a_text)
	McmHost.SetInfoText(a_text)
EndFunction

Function ForcePageReset()
	McmHost.ForcePageReset()
EndFunction

Function LoadCustomContent(String a_source, Float a_x = 0.0, Float a_y = 0.0)
	McmHost.LoadCustomContent(a_source, a_x, a_y)
EndFunction

Function UnloadCustomContent()
	McmHost.UnloadCustomContent()
EndFunction

; ---- options: kinds 0 empty, 1 header, 2 text, 3 toggle, 4 slider, 5 menu, 6 color, 7 key map, 8 input ----

Int Function AddEmptyOption()
	Return McmHost.AddOption(0, "", "", 0.0, 0, "")
EndFunction

Int Function AddHeaderOption(String a_text, Int a_flags = 0)
	Return McmHost.AddOption(1, a_text, "", 0.0, a_flags, "")
EndFunction

Int Function AddTextOption(String a_text, String a_value, Int a_flags = 0)
	Return McmHost.AddOption(2, a_text, a_value, 0.0, a_flags, "")
EndFunction

Int Function AddToggleOption(String a_text, Bool a_checked, Int a_flags = 0)
	Return McmHost.AddOption(3, a_text, "", _Flag(a_checked), a_flags, "")
EndFunction

Int Function AddSliderOption(String a_text, Float a_value, String a_formatString = "{0}", Int a_flags = 0)
	Return McmHost.AddOption(4, a_text, a_formatString, a_value, a_flags, "")
EndFunction

Int Function AddMenuOption(String a_text, String a_value, Int a_flags = 0)
	Return McmHost.AddOption(5, a_text, a_value, 0.0, a_flags, "")
EndFunction

Int Function AddColorOption(String a_text, Int a_color, Int a_flags = 0)
	Return McmHost.AddOption(6, a_text, "", a_color, a_flags, "")
EndFunction

Int Function AddKeyMapOption(String a_text, Int a_keyCode, Int a_flags = 0)
	Return McmHost.AddOption(7, a_text, "", a_keyCode, a_flags, "")
EndFunction

Int Function AddInputOption(String a_text, String a_value, Int a_flags = 0)
	Return McmHost.AddOption(8, a_text, a_value, 0.0, a_flags, "")
EndFunction

Function AddTextOptionST(String a_stateName, String a_text, String a_value, Int a_flags = 0)
	McmHost.AddOption(2, a_text, a_value, 0.0, a_flags, a_stateName)
EndFunction

Function AddToggleOptionST(String a_stateName, String a_text, Bool a_checked, Int a_flags = 0)
	McmHost.AddOption(3, a_text, "", _Flag(a_checked), a_flags, a_stateName)
EndFunction

Function AddSliderOptionST(String a_stateName, String a_text, Float a_value, String a_formatString = "{0}", Int a_flags = 0)
	McmHost.AddOption(4, a_text, a_formatString, a_value, a_flags, a_stateName)
EndFunction

Function AddMenuOptionST(String a_stateName, String a_text, String a_value, Int a_flags = 0)
	McmHost.AddOption(5, a_text, a_value, 0.0, a_flags, a_stateName)
EndFunction

Function AddColorOptionST(String a_stateName, String a_text, Int a_color, Int a_flags = 0)
	McmHost.AddOption(6, a_text, "", a_color, a_flags, a_stateName)
EndFunction

Function AddKeyMapOptionST(String a_stateName, String a_text, Int a_keyCode, Int a_flags = 0)
	McmHost.AddOption(7, a_text, "", a_keyCode, a_flags, a_stateName)
EndFunction

Function AddInputOptionST(String a_stateName, String a_text, String a_value, Int a_flags = 0)
	McmHost.AddOption(8, a_text, a_value, 0.0, a_flags, a_stateName)
EndFunction

; ---- option values (noUpdate only defers a redraw, which the engine does per frame) ----

Function SetOptionFlags(Int a_option, Int a_flags, Bool a_noUpdate = False)
	McmHost.SetOptionFlags(a_option, a_flags)
EndFunction

Function SetTextOptionValue(Int a_option, String a_value, Bool a_noUpdate = False)
	McmHost.SetOptionText(a_option, a_value)
EndFunction

Function SetToggleOptionValue(Int a_option, Bool a_checked, Bool a_noUpdate = False)
	McmHost.SetOptionNumber(a_option, _Flag(a_checked), "")
EndFunction

Function SetSliderOptionValue(Int a_option, Float a_value, String a_formatString = "{0}", Bool a_noUpdate = False)
	McmHost.SetOptionNumber(a_option, a_value, a_formatString)
EndFunction

Function SetMenuOptionValue(Int a_option, String a_value, Bool a_noUpdate = False)
	McmHost.SetOptionText(a_option, a_value)
EndFunction

Function SetColorOptionValue(Int a_option, Int a_color, Bool a_noUpdate = False)
	McmHost.SetOptionNumber(a_option, a_color, "")
EndFunction

Function SetKeyMapOptionValue(Int a_option, Int a_keyCode, Bool a_noUpdate = False)
	McmHost.SetOptionNumber(a_option, a_keyCode, "")
EndFunction

Function SetInputOptionValue(Int a_option, String a_value, Bool a_noUpdate = False)
	McmHost.SetOptionText(a_option, a_value)
EndFunction

; The ST setters' option: the named state's, else the current state's (SkyUI's GetStateOptionIndex).
Int Function _StateOption(String a_stateName)
	If a_stateName == ""
		a_stateName = GetState()
	EndIf
	Return McmHost.StateOption(a_stateName)
EndFunction

Function SetOptionFlagsST(Int a_flags, Bool a_noUpdate = False, String a_stateName = "")
	McmHost.SetOptionFlags(_StateOption(a_stateName), a_flags)
EndFunction

Function SetTextOptionValueST(String a_value, Bool a_noUpdate = False, String a_stateName = "")
	McmHost.SetOptionText(_StateOption(a_stateName), a_value)
EndFunction

Function SetToggleOptionValueST(Bool a_checked, Bool a_noUpdate = False, String a_stateName = "")
	McmHost.SetOptionNumber(_StateOption(a_stateName), _Flag(a_checked), "")
EndFunction

Function SetSliderOptionValueST(Float a_value, String a_formatString = "{0}", Bool a_noUpdate = False, String a_stateName = "")
	McmHost.SetOptionNumber(_StateOption(a_stateName), a_value, a_formatString)
EndFunction

Function SetMenuOptionValueST(String a_value, Bool a_noUpdate = False, String a_stateName = "")
	McmHost.SetOptionText(_StateOption(a_stateName), a_value)
EndFunction

Function SetColorOptionValueST(Int a_color, Bool a_noUpdate = False, String a_stateName = "")
	McmHost.SetOptionNumber(_StateOption(a_stateName), a_color, "")
EndFunction

Function SetKeyMapOptionValueST(Int a_keyCode, Bool a_noUpdate = False, String a_stateName = "")
	McmHost.SetOptionNumber(_StateOption(a_stateName), a_keyCode, "")
EndFunction

Function SetInputOptionValueST(String a_value, Bool a_noUpdate = False, String a_stateName = "")
	McmHost.SetOptionText(_StateOption(a_stateName), a_value)
EndFunction

; ---- dialogs: fields 0 start, 1 default, 2 minimum, 3 maximum, 4 interval ----

Function SetSliderDialogStartValue(Float a_value)
	McmHost.SetDialogNumber(0, a_value)
EndFunction

Function SetSliderDialogDefaultValue(Float a_value)
	McmHost.SetDialogNumber(1, a_value)
EndFunction

Function SetSliderDialogRange(Float a_minValue, Float a_maxValue)
	McmHost.SetDialogNumber(2, a_minValue)
	McmHost.SetDialogNumber(3, a_maxValue)
EndFunction

Function SetSliderDialogInterval(Float a_value)
	McmHost.SetDialogNumber(4, a_value)
EndFunction

Function SetMenuDialogStartIndex(Int a_value)
	McmHost.SetDialogNumber(0, a_value)
EndFunction

Function SetMenuDialogDefaultIndex(Int a_value)
	McmHost.SetDialogNumber(1, a_value)
EndFunction

Function SetMenuDialogOptions(String[] a_options)
	McmHost.SetMenuDialogOptions(a_options)
EndFunction

Function SetColorDialogStartColor(Int a_color)
	McmHost.SetDialogNumber(0, a_color)
EndFunction

Function SetColorDialogDefaultColor(Int a_color)
	McmHost.SetDialogNumber(1, a_color)
EndFunction

Function SetInputDialogStartText(String a_text)
	McmHost.SetInputDialogText(a_text)
EndFunction

; Blocks until the player answers (SkyUI waits in menu mode). The VM cannot suspend inside a call for
; a value yet, so the checker declines a script that calls it (docs/engine/mcm-host.md).
Bool Function ShowMessage(String a_message, Bool a_withCancel = True, String a_acceptLabel = "$Accept", String a_cancelLabel = "$Cancel")
	Return McmHost.ShowMessage(a_message, a_withCancel, a_acceptLabel, a_cancelLabel)
EndFunction

Float Function _Flag(Bool a_value)
	If a_value
		Return 1.0
	EndIf
	Return 0.0
EndFunction

; ---- the engine's entry: one panel action, dispatched as SkyUI does (a state option's event runs in
; its state, then the previous state is restored) ----

Function _McmEvent(Int a_action, Int a_option, Float a_number, String a_text)
	If a_action == 0
		CurrentPage = a_text
		OnPageReset(a_text)
		Return
	ElseIf a_action == 13
		; Its name and pages are its properties: the menu reads them again as it opens.
		McmHost.Register(ModName, Pages)
		OnConfigOpen()
		Return
	ElseIf a_action == 14
		OnConfigClose()
		Return
	EndIf
	String optionState = McmHost.OptionState(a_option)
	If optionState != ""
		String oldState = GetState()
		GotoState(optionState)
		_McmStateEvent(a_action, a_number, a_text)
		GotoState(oldState)
	Else
		_McmOptionEvent(a_action, a_option, a_number, a_text)
	EndIf
EndFunction

Function _McmStateEvent(Int a_action, Float a_number, String a_text)
	If a_action == 1
		OnHighlightST()
	ElseIf a_action == 2
		OnDefaultST()
	ElseIf a_action == 3
		OnSelectST()
	ElseIf a_action == 4
		OnSliderOpenST()
	ElseIf a_action == 5
		OnSliderAcceptST(a_number)
	ElseIf a_action == 6
		OnMenuOpenST()
	ElseIf a_action == 7
		OnMenuAcceptST(a_number as Int)
	ElseIf a_action == 8
		OnColorOpenST()
	ElseIf a_action == 9
		OnColorAcceptST(a_number as Int)
	ElseIf a_action == 10
		OnKeyMapChangeST(a_number as Int, a_text, "")
	ElseIf a_action == 11
		OnInputOpenST()
	ElseIf a_action == 12
		OnInputAcceptST(a_text)
	EndIf
EndFunction

Function _McmOptionEvent(Int a_action, Int a_option, Float a_number, String a_text)
	If a_action == 1
		OnOptionHighlight(a_option)
	ElseIf a_action == 2
		OnOptionDefault(a_option)
	ElseIf a_action == 3
		OnOptionSelect(a_option)
	ElseIf a_action == 4
		OnOptionSliderOpen(a_option)
	ElseIf a_action == 5
		OnOptionSliderAccept(a_option, a_number)
	ElseIf a_action == 6
		OnOptionMenuOpen(a_option)
	ElseIf a_action == 7
		OnOptionMenuAccept(a_option, a_number as Int)
	ElseIf a_action == 8
		OnOptionColorOpen(a_option)
	ElseIf a_action == 9
		OnOptionColorAccept(a_option, a_number as Int)
	ElseIf a_action == 10
		OnOptionKeyMapChange(a_option, a_number as Int, a_text, "")
	ElseIf a_action == 11
		OnOptionInputOpen(a_option)
	ElseIf a_action == 12
		OnOptionInputAccept(a_option, a_text)
	EndIf
EndFunction
