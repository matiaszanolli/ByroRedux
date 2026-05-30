# #1327 — OBL-D4-NEW-03 dead NiPSysBlock legacy-particle arms

_Filed + fixed 23ab46f2 (orphaned from the #1308 publish crossover)._

Dead by type mismatch: legacy types dispatch to legacy_particle::*, not NiPSysBlock. Deleted both walk arms + corrected comments + retargeted the fiction test.
