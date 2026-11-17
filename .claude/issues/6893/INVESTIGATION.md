# #6893 — disassembly pass (2026-11-16), NOT settled

Binary: `Fallout3.exe` (Steam GOTY, base 0x400000, .text unencrypted). Tooling: python3 -I + capstone 5,
a one-file PE/xref helper (strings, data xrefs, E8 callers, disassembly). 12 tool calls, time-boxed.

## Ruled out (addresses are Fallout3.exe VAs)
- `fActorTeleportFadeSeconds` (GMST value read only at 0x860cfe): the screen-fade timer for a teleport, not placement.
- "Travel %f units" / "--Estimated Travel Time: %.2f game hours." (0xf7cf2c / 0xf7cff8, code 0x78fa48..0x790198, function start ~0x78f590, reached through a vtable slot at 0x11112f0): a debug description of a Travel package. It does use distance / speed, so it is a hint for XCT-OFF-001's walk-time model, but it places nothing.
- `bFixAIPackagesOnLoad:General` (0xf68f54): the setting-object layout was misread; the code at 0x6e7c00 is gated on a different setting. Not usable without re-deriving the INI-setting struct layout.

## Found, relevant but not decisive
- 0xf931a0 "PATHFINDING: Actor %s used teleport door %s in cell %s and was not moved to the expected pathing location, check teleport marker". Code: function ~0x8ac230 (one caller, 0x8abfe3), message built at 0x8ac3a8. This is the *loaded* actor door-use check: after a door transition the actor is expected at the door's teleport marker (the far side). It is evidence for XCT-OFF-004's "far-side door" for an actor that is *walking*, not for an unwatched arrival.
- CORRECTION (second pass): `0x755f70` is `TESObjectREFR::Activate`, not a move routine. It contains the string "'%s' activated %s '%s'" (pass 2) and its 48 callers are AI door/furniture/lockpicking code (0x751b30, 0x7529d0, 0x88bd50..0x88c810 furniture exits, 0x8ac230 door use, 0x942430). It does not place actors.
- RTTI: `TeleportDoorSearch`, `AStarSearch<ParentSpaceNode, TeleportLink>` exist (type descriptors 0x120c40c / 0x120c430), so cross-space routing is an A* over teleport links. My RTTI-to-vtable walk returned class-hierarchy data, not vtables; do not trust the vtable addresses 0x1055e50 / 0x1055e54.

## Not found
No string names the unloaded-actor arrival or a "package location" placement (FO3 has no `MoveToPackageLocation`). The relevant code has no string anchors, so it needs a callers walk from 0x755f70 or from the A* result consumer, or an in-game observation.

## Second pass (callers of 0x755f70, then the MoveTo command)
- The script `MoveToMarker`/`MoveTo` command entry is at 0x110cc38; its execute function is 0x79ba90, which parses arguments (0x787530) and, in the body at 0x79bc20, either builds a deferred move request for the player (pool alloc at 0x11165dc, size 0x34) or calls a virtual (vtable slot +0x1f4 returns a position) and a routine at 0x75e540 (229 callers, too many to triage by hand). That is the script path, not the unloaded-actor path.
- Not done: correcting the RTTI walk (filter COL candidates by signature 0 and pTypeDescriptor at COL+12 == the type descriptor), then listing the LowProcess / MiddleLowProcess vtable slots (type descriptors 0x1204da4 / 0x1204d84) and looking for the slot that moves an actor to a package location.

## Next, if continuing
1. `callers(0x755f70)` and filter to functions that also touch a Package or a LowProcess/MiddleLowProcess object (RTTI names exist at 0x1204d84 / 0x1204da4).
2. Find the consumer of the `TeleportDoorSearch` result (xref the AStarSearch<ParentSpaceNode,TeleportLink> constructor by the 0x1055e50-area data, after fixing the RTTI walk).
3. Otherwise the original-game observation in the issue stands.

## Third pass: correct vtables and the LowProcess scan (2026-11-16), still NOT settled

RTTI walk fixed: a complete-object-locator candidate is a dword D that points at an object whose first dword is 0 and whose dword at +12 is the type descriptor (name VA - 8); the vtable is the address of the dword that holds D, plus 4. Process vtables (all ~500 slots, 0x7cc is slot 499): BaseProcess 0xf896c0, HighProcess 0xf8ff64, LowProcess 0xf90a7c, MiddleHighProcess 0xf91358, MiddleLowProcess 0xf91e08.

Findings:
- `ExtraPackageStartLocation` is extra type 0x18 (cell/space at +0xC, position at +0x10..0x18, rotation at +0x1C; ctor 0x5e4fa0). `ExtraPackage` is type 0x19 (ctor 0x5e4f30). Setter `0x5e16d0`; REFR wrapper `0x76bb00` (stores the extra, then marks change flag 0x800, so it is saved).
- The wrapper is called by `0x882c00` (Low-process region). That routine records the actor's CURRENT cell, position and rotation as its package start location (vcall +0x1f4 is GetPos, 0x75e540 reads the parent space from [ref+0x3c]). So "package start location" is where the actor was when the package began, not where a Travel package ends. It is not the arrival point.
- LowProcess and MiddleLowProcess share their package slots (0x7cc -> 0x881310, 0x8c -> 0x882d50) and differ from HighProcess (0x7cc -> 0x86b2d0, 0x8c -> 0x894050). `0x882d90` is a Low-process package-start routine: it fetches the package list at [actor+0x40], switches on the package type byte at [package+0x20] (jump-table idiom `movzx ecx, byte [ecx+0x882f90]; jmp [ecx*4+0x882f88]`, 18 cases), sets process flags through vcalls +0x598/+0x5a0/+0x4e4/+0x118, picks a package location through process vcalls +0x7f8 / +0x7fc, and stores it with 0x5e1520. I found no position write in it.
- The jump table at 0x882f88 did not decode as pointers (the bytes at 0x882f88.. mix a byte table and a pointer table, so the dispatch is two-level: index byte at 0x882f90, then pointers at 0x882f88 + 4*index). Re-read it as such before trusting any case.

Not found: any branch that moves an unloaded actor to a destination cell or to a package location. The arrival is likely not in a Low-process slot at all (the 120-second processing cadence from the GECK page points at a per-actor update driven from the cell/process manager).

Suggested next, if continued: decode process slots +0x7f8 / +0x7fc (the package-location computation) and find what drives LowProcess updates (the owner of the NiTMap<LowProcess*> at RTTI 0x121062c); otherwise take the in-game observation.
