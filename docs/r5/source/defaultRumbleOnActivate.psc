ScriptName defaultRumbleOnActivate Extends objectreference
{ Quick script that shakes camera and/or controller on activation.  Customizable via properties }

;-- Variables ---------------------------------------

;-- Properties --------------------------------------
Float Property cameraIntensity = 0.25 Auto
{ How hard to shake camera, range:0-1 }
Float Property duration = 0.25 Auto
{ how long to shake controller }
Bool Property repeatable = True Auto
{ by default, this happens per activation }
Float Property shakeLeft = 0.25 Auto
{ How hard to shake left motor, range:0-1 }
Float Property shakeRight = 0.25 Auto
{ How hard to shake right motor, range:0-1 }

;-- Functions ---------------------------------------

; Skipped compiler generated GetState

; Skipped compiler generated GotoState

;-- State -------------------------------------------
Auto State active

  Event onActivate(ObjectReference actronaut)
    Game.shakeCamera(None, cameraIntensity, 0.0) ; #DEBUG_LINE_NO:23
    Game.shakeController(shakeLeft, shakeRight, duration) ; #DEBUG_LINE_NO:24
    Self.GotoState("busy") ; #DEBUG_LINE_NO:25
    Utility.wait(duration) ; #DEBUG_LINE_NO:26
    If repeatable == True ; #DEBUG_LINE_NO:27
      Self.GotoState("active") ; #DEBUG_LINE_NO:28
    Else
      Self.GotoState("inactive") ; #DEBUG_LINE_NO:30
    EndIf
  EndEvent
EndState

;-- State -------------------------------------------
State busy

  Event onActivate(ObjectReference actronaut)
    ; Empty function
  EndEvent
EndState

;-- State -------------------------------------------
State inactive
EndState
