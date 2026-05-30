# #1328 — FO3/FNV/Skyrim+ WRLD DNAM default water

_Filed + fixed 1ea828dd (#1305 follow-up)._

WRLD DNAM = [land f32, water f32] (8B, verified FNV+Skyrim). Parse offset-4 water height → WorldspaceRecord.default_water_height; non-Oblivion fallback uses it, Oblivion stays Z=0.
