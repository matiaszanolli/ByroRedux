ScriptName DA10MainDoorScript Extends ReferenceAlias

;-- Functions ---------------------------------------

; Skipped compiler generated GetState

; Skipped compiler generated GotoState

Event OnActivate(ObjectReference akActionRef)
  If (Self.GetOwningQuest().GetStageDone(37) == 1 as Bool) && (Self.GetOwningQuest().GetStageDone(40) == 0 as Bool) ; #DEBUG_LINE_NO:13
    Self.GetOwningQuest().SetStage(40) ; #DEBUG_LINE_NO:14
  EndIf
EndEvent
