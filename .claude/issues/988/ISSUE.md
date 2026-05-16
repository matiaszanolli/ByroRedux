# SK-D5-NEW-09: BSLODTriShape geometry silently dropped by import walker

**GitHub**: #988
**Domain**: nif (import walker)
**Severity**: MEDIUM

## Root Cause
#838 added NiLodTriShape block type but never added a downcast arm in the two 
import walkers (walk_node_local line 328-430, walk_node_flat line 741-805).
BSLODTriShape → NiLodTriShape with .base: NiTriShape. 23 Skyrim SE meshes affected.

## Fix (2 files)
1. walk.rs: add NiLodTriShape to imports, add arm in walk_node_local + walk_node_flat
   Body: same as NiTriShape arm but operating on &lod.base
2. dispatch_tests or walk_tests: add regression test
