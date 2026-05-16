ScriptName MG07LabyrinthianDoorSCRIPT Extends ObjectReference
{ Handling script for the main lockout door on Labyrinthian to prevent access prior to MG07 }

;-- Variables ---------------------------------------
Bool beenOpened = False

;-- Properties --------------------------------------
Quest Property MG07 Auto
MiscObject Property MG07Keystone Auto
Float Property delayAfterInsert = 1.0 Auto
Message Property dunLabyrinthianDenialMSG Auto
ObjectReference Property myDoor Auto

;-- Functions ---------------------------------------

; Skipped compiler generated GetState

; Skipped compiler generated GotoState

Event onLoad()
  If beenOpened == False ; #DEBUG_LINE_NO:13
    Self.blockActivation(True) ; #DEBUG_LINE_NO:14
  Else
    Self.disable(False) ; #DEBUG_LINE_NO:16
  EndIf
  Self.GotoState("waiting") ; #DEBUG_LINE_NO:19
EndEvent

;-- State -------------------------------------------
State inactive
EndState

;-- State -------------------------------------------
State waiting

  Event onActivate(ObjectReference actronaut)
    If (actronaut == Game.getPlayer() as ObjectReference) && MG07.getStageDone(10) == True && Game.getPlayer().getItemCount(MG07Keystone as Form) >= 1 ; #DEBUG_LINE_NO:24
      Self.GotoState("inactive") ; #DEBUG_LINE_NO:25
      Game.getPlayer().removeItem(MG07Keystone as Form, Game.getPlayer().getItemCount(MG07Keystone as Form), False, None) ; #DEBUG_LINE_NO:26
      Self.playAnimationAndWait("Insert", "Done") ; #DEBUG_LINE_NO:27
      beenOpened == False ; #DEBUG_LINE_NO:28
      Utility.wait(delayAfterInsert) ; #DEBUG_LINE_NO:29
      Self.disable(False) ; #DEBUG_LINE_NO:30
      myDoor.activate(actronaut, False) ; #DEBUG_LINE_NO:31
    Else
      dunLabyrinthianDenialMSG.show(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0) ; #DEBUG_LINE_NO:34
    EndIf
  EndEvent
EndState
