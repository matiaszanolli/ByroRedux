# #1583 F3 (renderer) + #1590 FO4-D1-MED-01 (binary/nif)

## #1583 — write-only gb_reservoir attachment, dead VRAM (~66MB@1080p)
Gate/remove the 7th G-buffer attachment (R32G32B32A32_UINT) until ReSTIR resample pass exists.

## #1590 — DLC precombines resolve wrong CSG + remapped path form-id
(a) open_geometry_csg keys off plugin stem, ignores BSPackedGeomObject.filename_hash
(b) _oc.nif path uses remapped global form_id, not plugin-local
+ index-range guard in decode_shared_geom_object (fail closed)
