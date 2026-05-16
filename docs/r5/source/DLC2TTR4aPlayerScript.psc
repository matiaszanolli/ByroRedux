ScriptName DLC2TTR4aPlayerScript Extends ReferenceAlias

;-- Variables ---------------------------------------

;-- Properties --------------------------------------
Quest Property DLC2TTR4a Auto

;-- Functions ---------------------------------------

; Skipped compiler generated GetState

; Skipped compiler generated GotoState

Event OnInit()
  Self.RegisterForUpdate(5 as Float) ; #DEBUG_LINE_NO:6
EndEvent

Event OnUpdate()
  If Game.GetPlayer().GetActorValue("Variable05") > 0 as Float ; #DEBUG_LINE_NO:10
    DLC2TTR4a.SetStage(200) ; #DEBUG_LINE_NO:11
    Self.UnregisterForUpdate() ; #DEBUG_LINE_NO:12
  EndIf
EndEvent
