//! World-definition records — navigation, regions, encounter zones,
//! lighting templates, image-space adapters, activators, terminals.

use super::super::common::{read_zstring, remap_fid, CommonNamedFields};
use crate::esm::reader::{FormIdRemap, SubRecord};
use crate::esm::sub_reader::SubReader;
use std::collections::HashMap;

/// Navigation mesh master record (`NAVI`). Skyrim+ splits navigation
/// metadata into a top-level master + per-cell `NAVM` children; for
/// pre-Skyrim games this is rare but still present on wilderness
/// worldspaces. Post-render, not a draw path.
#[derive(Debug, Clone, Default)]
pub struct NaviRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `NVER` version tag — format revision the mesh data was exported at.
    pub version: u32,
}

pub fn parse_navi(form_id: u32, subs: &[SubRecord]) -> NaviRecord {
    let mut out = NaviRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs(subs);
    out.editor_id = common.editor_id;
    for sub in subs {
        match &sub.sub_type {
            b"NVER" if sub.data.len() >= 4 => {
                out.version = SubReader::new(&sub.data).u32_or_default();
            }
            _ => {}
        }
    }
    out
}

/// Navmesh (`NAVM`) — the walkable surface for one cell.
///
/// Two structurally different encodings exist in the lineage and both land in
/// this one type; see [`parse_navm`] for how they were established.
#[derive(Debug, Clone, Default)]
pub struct NavmRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `NVER` version tag — format revision the mesh data was exported at.
    pub version: u32,
    /// `DATA` word 0 — the `CELL` this mesh belongs to. `None` on the packed
    /// Creation-Engine form, which authors no `DATA`.
    pub cell_form: Option<u32>,
    /// `NVVX` vertices in world units (stride 12 = three `f32`).
    pub vertices: Vec<[f32; 3]>,
    /// `NVTR` triangles (stride 16).
    pub triangles: Vec<NavmTriangle>,
    /// `NVEX` links to triangles in neighbouring meshes (stride 10).
    pub external_connections: Vec<NavmExternalConnection>,
    /// Worldspace this tile belongs to, from the `NVNM` header. `0` means the
    /// tile is interior — [`cell_form`](Self::cell_form) then names the cell.
    /// `None` on the Gamebryo typed form, which authors no worldspace word.
    pub worldspace_form: Option<u32>,
    /// Exterior cell grid `(x, y)` this tile belongs to, from the `NVNM`
    /// header. Present only when `worldspace_form` is a non-zero worldspace;
    /// interiors carry [`cell_form`](Self::cell_form) instead. See
    /// [`parse_navm`] for how the branch and the axis order were established.
    pub grid: Option<(i32, i32)>,
    /// Creation-Engine `NVNM` bytes, retained when the body could not be
    /// decoded.
    ///
    /// `Some` for Fallout 4, whose body layout diverges from Skyrim's after
    /// the shared header (measured: 0 of 7,894 `Fallout4.esm` blobs reconcile
    /// under the Skyrim body layout; OpenMW skips the FO4 form for the same
    /// reason). Kept verbatim so a later decoder has the bytes without a
    /// re-parse, and so a consumer can tell "packed form, not yet decoded"
    /// from "empty". `None` once the body decoded into the typed fields above.
    pub packed_geometry: Option<Vec<u8>>,
    /// Triangles that sit in a door's threshold, with the door reference that
    /// governs them — `NVDP` on the Gamebryo typed form (stride 8), the
    /// fourth counted list on the packed `NVNM` form (stride 10). #3300.
    pub door_triangles: Vec<NavmDoorTriangle>,
    /// Triangle indices flagged as combat cover — `NVCA` on the typed form,
    /// the fifth counted list on the packed form. Both are bare `u16`
    /// indices into [`triangles`](Self::triangles). #3300.
    pub cover_triangles: Vec<u16>,
    /// `NVGD` — the typed form's grid acceleration structure: a
    /// `divisor x divisor` lattice over the mesh's bounds, each cell listing
    /// the triangles overlapping it. `None` when absent or unrecognised.
    /// #3300.
    pub grid_accel: Option<NavmGridAccel>,
}

/// One `NVDP` row — a door-governed navmesh triangle.
///
/// #3300 — layout established by census over the shipped Gamebryo corpus
/// (`Fallout3.esm` 7,198 meshes / `FalloutNV.esm` 4,771; 1,113 + 1,100 rows):
/// stride is **8**, not the packed form's 10, and `DATA` word 5 equals the
/// row count on every one of the 11,969 meshes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NavmDoorTriangle {
    /// The door `REFR` this threshold belongs to.
    pub door_form_id: u32,
    /// Index into [`NavmRecord::triangles`]. In range on 1,113/1,113 FO3 rows
    /// and 1,099/1,100 FNV rows.
    pub triangle: u16,
    /// Trailing field, semantics **unresolved** — deliberately not named.
    /// Zero on 50.8% of FO3 rows and 70.9% of FNV rows, and of the non-zero
    /// remainder only 72.1% / 56.9% fall inside the triangle range, so it is
    /// not a second triangle index. Captured raw so a later investigation has
    /// it; same posture as [`NavmExternalConnection::unknown`].
    pub unknown: u32,
}

/// `NVGD` — grid acceleration lattice over a navmesh's bounds.
///
/// #3300 — `u32 divisor`, then eight `f32` (max X/Y step, then min/max XYZ
/// bounds), then `divisor^2` lists of `u16` triangle indices, **each prefixed
/// by a `u16` count** — the one place this diverges from the packed `NVNM`
/// tail block, which uses a `u32` count. Established by exact reconciliation:
/// 11,969 of 11,969 shipped meshes consume their payload to the byte, across
/// 5,190,668 triangle references with **zero** out of range.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NavmGridAccel {
    /// Lattice is `divisor x divisor` cells. Observed 1..=12 on shipped data.
    pub divisor: u32,
    /// Max X and Y step, then min XYZ and max XYZ of the mesh bounds.
    pub bounds: [f32; 8],
    /// One triangle-index list per lattice cell, row-major, `divisor^2` long.
    pub cells: Vec<Vec<u16>>,
}

impl NavmRecord {
    /// True when the geometry is present in decoded form.
    pub fn has_decoded_geometry(&self) -> bool {
        !self.vertices.is_empty() && !self.triangles.is_empty()
    }

    /// FormIDs of every mesh this one links to, deduplicated.
    ///
    /// The connectivity query EX-16 needs: which neighbouring meshes must be
    /// resident for a path to leave this one.
    pub fn linked_meshes(&self) -> Vec<u32> {
        let mut out: Vec<u32> = self
            .external_connections
            .iter()
            .map(|c| c.mesh_form)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// True when every triangle's vertex indices are in range.
    ///
    /// Holds for all 11,969 FO3 + FNV meshes, so a failure means either a
    /// corrupt plugin or a decode regression.
    pub fn indices_are_in_range(&self) -> bool {
        let limit = self.vertices.len();
        self.triangles
            .iter()
            .all(|t| t.vertices.iter().all(|&v| (v as usize) < limit))
    }
}

/// One navmesh triangle: three vertex indices, three edge neighbours, flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavmTriangle {
    /// Indices into [`NavmRecord::vertices`].
    pub vertices: [u16; 3],
    /// Triangle index across each edge, or `None` where the edge is a border.
    /// `0xFFFF` is the authored sentinel for "no neighbour".
    pub edge_neighbours: [Option<u16>; 3],
    /// Per-triangle flags (preferred pathing, water, door, …).
    pub flags: u32,
}

/// A link from a triangle in this mesh to a triangle in another `NAVM`.
///
/// This is the record that makes cross-cell pathing possible: without it a
/// mesh is an island and a path cannot leave its own cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavmExternalConnection {
    /// Leading word. Small (1, 2, …) and not established by the corpus, so it
    /// is carried rather than interpreted.
    pub unknown: u32,
    /// FormID of the `NAVM` on the other side of the link.
    pub mesh_form: u32,
    /// Triangle index within that mesh.
    pub triangle: u16,
}

/// Parse a navmesh (`NAVM`), decoding geometry and cross-cell links (#2738).
///
/// # Two encodings, established from shipped data
///
/// Sweeping `Oblivion.esm`, `Fallout3.esm`, `FalloutNV.esm` and `Skyrim.esm`
/// shows the lineage splits cleanly:
///
/// - **Oblivion authors no `NAVM` at all** (0 records) — it used a different
///   pathing scheme, so the canonical model must tolerate absence rather than
///   assume a mesh exists.
/// - **Gamebryo (FO3 7,198 / FNV 4,771)** uses typed sub-records: `DATA`
///   (24 B), `NVVX` vertices (stride 12), `NVTR` triangles (stride 16),
///   `NVEX` external links (stride 10), plus `NVDP`/`NVCA`/`NVGD`.
/// - **Creation (Skyrim 15,966)** packs everything into one `NVNM` blob
///   (130 B – 97 KB) with `ONAM`/`PNAM`/`NNAM` alongside.
///
/// Strides were derived by taking the GCD of every observed payload length,
/// then confirmed against the `DATA` header: words 1, 2 and 3 equal the
/// vertex, triangle and external-connection counts for **all 11,969** FO3 and
/// FNV meshes, and **no triangle references an out-of-range vertex** in any of
/// them. `DATA` word 0 is the parent `CELL` FormID.
///
/// Words 4 and 5 are no longer undecoded (#3300): word 4 equals the `NVCA`
/// cover-triangle count and word 5 the `NVDP` door-triangle count, on every
/// one of the same 11,969 meshes. That also settles the two strides — `NVCA`
/// is 2 (bare `u16` triangle indices) and `NVDP` is **8**, not the packed
/// form's 10.
///
/// `NVTR`'s 16 bytes are three `u16` vertex indices, three `u16` edge
/// neighbours and a `u32` flag word, with `0xFFFF` marking a border edge.
/// `NVEX`'s 10 bytes carry a `u32`, the neighbouring mesh's FormID, and a
/// `u16` triangle index.
///
/// # The Creation-Engine `NVNM` blob (#2738)
///
/// One length-prefixed stream rather than typed sub-records. Field order is
/// OpenMW's `components/esm4/loadnavm.cpp`; the two decisions that reference
/// gets wrong or leaves uncertain were settled against shipped data by
/// [`decode_nvnm`], which is also why the decode is gated on exact
/// reconciliation rather than on a version check.
///
/// ```text
/// u32 unknown0, u32 unknown1, u32 worldspace
/// worldspace != 0 ? (i16 grid_y, i16 grid_x) : u32 cell_form
/// u32 n; [f32; 3] * n         vertices
/// u32 n; 16 B    * n          triangles (same 16-byte row as NVTR)
/// u32 n; 10 B    * n          external connections (same row as NVEX)
/// u32 n; 10 B    * n          door triangles      — stored, #3300
/// u32 n; u16     * n          cover triangles     — stored, #3300
/// u32 divisor, 8 * f32 bounds, then divisor^2 counted u16 index lists
/// ```
///
/// Door and cover triangles are now **stored** (#3300) into the same
/// [`NavmRecord::door_triangles`] / [`NavmRecord::cover_triangles`] fields the
/// Gamebryo `NVDP`/`NVCA` sub-records fill, so a consumer never branches per
/// game. The packed door row orders its fields differently from `NVDP` — see
/// the census note at the capture site. The bounds block and segment lists are
/// still walked but not stored on this form; the Gamebryo form's equivalent
/// (`NVGD`) is decoded into [`NavmRecord::grid_accel`].
///
/// **Fallout 4 keeps its blob.** The header decodes there (verified below),
/// but 0 of 7,894 `Fallout4.esm` bodies reconcile under this layout; OpenMW
/// skips the FO4 form for the same reason. Rather than version-gate, the
/// decoder simply requires `consumed == len` and falls back to retaining the
/// bytes, which also protects against a future format revision.
///
/// # Evidence for the header branch
///
/// Word 3 is four bytes either way, so a length check cannot discriminate
/// grid-vs-cell — both candidate rules reconcile 15,966/15,966 on
/// `Skyrim.esm`. The owning `CELL` settles it independently, since a `NAVM`
/// sits in that cell's child group whose header label is the cell FormID:
///
/// | worldspace word | tiles | word 3 == owning CELL |
/// |---|---:|---:|
/// | `0` (interior) | 1,526 | **1,526** |
/// | `0x3C` (Tamriel) | 12,817 | 0 |
/// | any other worldspace | 1,623 | 0 |
///
/// So the rule is `worldspace != 0 → grid`, not OpenMW's hardcoded
/// `worldspace == 0x3C` (whose own FIXME concedes the check is wrong). The
/// difference is not academic: it misreads every one of Solstheim's 1,540
/// `Dragonborn.esm` tiles as a cell FormID.
///
/// Axis order likewise comes from the data, not the comment: compared against
/// the owning exterior cell's `XCLC`, `(y, x)` matches 14,440/14,440 on
/// `Skyrim.esm` and 1,564/1,564 on `Dragonborn.esm`, while `(x, y)` matches
/// only the diagonal coincidences (210 and 59).
///
/// # Decode coverage (`examples/dump_navmesh`, 2026-08-13)
///
/// | Plugin | `NAVM` | geometry decoded | blob kept | cell/grid known |
/// |---|---:|---:|---:|---:|
/// | `Skyrim.esm` | 15,966 | 15,966 | 0 | 15,966 |
/// | `Dragonborn.esm` | 1,723 | 1,722 | 0 | 1,722 |
/// | `Dawnguard.esm` | 1,922 | 1,863 | 0 | 1,863 |
/// | `Fallout3.esm` | 7,198 | 7,198 | 0 | 7,198 |
/// | `FalloutNV.esm` | 4,771 | 4,771 | 0 | 4,771 |
/// | `Fallout4.esm` | 7,894 | 0 | 7,894 | **7,894** |
/// | `Oblivion.esm` | 0 | — | — | — |
///
/// Every triangle in every decoded mesh indexes a vertex that exists
/// (`indices_are_in_range`), across all 31,520 of them.
///
/// The shortfalls are accounted for, not unexplained: the 59 Dawnguard and 1
/// Dragonborn tiles that yield nothing are **deleted-record overrides**
/// (header flag `0x20`) carrying no sub-records at all, so there is nothing
/// to decode. Fallout 4 keeps every blob but still locates every tile,
/// because its divergence begins after the shared header.
pub fn parse_navm(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> NavmRecord {
    let mut out = NavmRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    out.editor_id = common.editor_id;
    // `DATA`'s words 1-5 are counts that reconcile with the typed
    // sub-records' own row counts on every one of 11,969 sampled FO3+FNV
    // meshes (see this fn's doc comment) — read once, up front, since
    // `DATA` isn't guaranteed to precede the sub-records it cross-checks
    // in `subs`' order (#3404).
    let data_words: Option<[u32; 6]> = subs
        .iter()
        .find(|s| s.sub_type == *b"DATA")
        .filter(|s| s.data.len() >= 24)
        .map(|s| std::array::from_fn(|i| u32_at(&s.data, i * 4).unwrap_or(0)));
    for sub in subs {
        let data = &sub.data;
        match &sub.sub_type {
            b"NVER" if data.len() >= 4 => {
                out.version = SubReader::new(data).u32_or_default();
            }
            b"DATA" if data.len() >= 24 => {
                out.cell_form = u32_at(data, 0);
            }
            // #3404 — `rows_exact` refuses a stride remainder instead of
            // silently truncating it away like `rows()` does, and the
            // result is additionally discarded unless its row count also
            // matches `DATA`'s own count for this sub-record — the same
            // all-or-nothing posture `decode_nvgd`/`decode_nvnm` already
            // take, so a format revision surfaces as "not recognised"
            // rather than a half-filled lattice.
            b"NVVX" => {
                out.vertices = rows_exact(data, 12)
                    .map(|it| {
                        it.map(|r| {
                            [
                                f32::from_le_bytes([r[0], r[1], r[2], r[3]]),
                                f32::from_le_bytes([r[4], r[5], r[6], r[7]]),
                                f32::from_le_bytes([r[8], r[9], r[10], r[11]]),
                            ]
                        })
                        .collect::<Vec<_>>()
                    })
                    .filter(|v| data_words.is_none_or(|w| w[1] as usize == v.len()))
                    .unwrap_or_default();
            }
            // Same 16-byte row the packed `NVNM` body carries (#2738), so
            // both forms go through one decoder.
            b"NVTR" => {
                out.triangles = rows_exact(data, 16)
                    .map(|it| it.map(decode_nvtr_row).collect::<Vec<_>>())
                    .filter(|v| data_words.is_none_or(|w| w[2] as usize == v.len()))
                    .unwrap_or_default();
            }
            b"NVEX" => {
                out.external_connections = rows_exact(data, 10)
                    .map(|it| it.filter_map(decode_nvex_row).collect::<Vec<_>>())
                    .filter(|v| data_words.is_none_or(|w| w[3] as usize == v.len()))
                    .unwrap_or_default();
            }
            // #3300 — door / cover / grid, previously unwalked on the typed
            // form. Strides come from a census of the shipped Gamebryo corpus,
            // cross-checked against `DATA`: word 4 equals the `NVCA` count and
            // word 5 the `NVDP` count on all 11,969 FO3+FNV meshes — now
            // enforced at decode time, not just documented (#3404).
            b"NVDP" => {
                out.door_triangles = rows_exact(data, 8)
                    .map(|it| it.map(decode_nvdp_row).collect::<Vec<_>>())
                    .filter(|v| data_words.is_none_or(|w| w[5] as usize == v.len()))
                    .unwrap_or_default();
            }
            b"NVCA" => {
                out.cover_triangles = rows_exact(data, 2)
                    .map(|it| {
                        it.map(|r| u16::from_le_bytes([r[0], r[1]]))
                            .collect::<Vec<_>>()
                    })
                    .filter(|v| data_words.is_none_or(|w| w[4] as usize == v.len()))
                    .unwrap_or_default();
            }
            b"NVGD" => out.grid_accel = decode_nvgd(data),
            // #2738 — decode the Creation-Engine packed form into the same
            // canonical fields the Gamebryo typed form fills, so consumers
            // never branch per game. Retains the blob when the body doesn't
            // reconcile (Fallout 4).
            b"NVNM" => decode_nvnm(data, &mut out),
            _ => {}
        }
    }
    // #3401 — every cross-record reference this walk collected is a
    // FormID into the same global space `EsmIndex`'s maps are keyed by.
    // Applied here rather than inside each row decoder because the typed
    // sub-records (`DATA` / `NVEX` / `NVDP`) and the packed `NVNM` blob
    // reach the same fields through two independent paths; one post-pass
    // cannot miss a branch the way four threading points could. Latent
    // until #2372 / EX-16 consumes the cross-tile joins and door
    // associations, which is exactly when a wrong key would start to
    // cost something.
    out.worldspace_form = out.worldspace_form.map(|f| remap_fid(f, remap));
    out.cell_form = out.cell_form.map(|f| remap_fid(f, remap));
    for conn in &mut out.external_connections {
        conn.mesh_form = remap_fid(conn.mesh_form, remap);
    }
    for door in &mut out.door_triangles {
        door.door_form_id = remap_fid(door.door_form_id, remap);
    }
    out
}

/// Cursor over an `NVNM` blob that refuses to read past the end.
///
/// Every accessor returns `None` on overrun so a wrong branch or a future
/// format revision surfaces as "did not reconcile" instead of garbage.
struct NvnmCursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> NvnmCursor<'a> {
    fn u32(&mut self) -> Option<u32> {
        let raw = self.data.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(u32::from_le_bytes(raw.try_into().ok()?))
    }

    fn i16(&mut self) -> Option<i16> {
        let raw = self.data.get(self.pos..self.pos + 2)?;
        self.pos += 2;
        Some(i16::from_le_bytes(raw.try_into().ok()?))
    }

    /// A `u32` count followed by `count * stride` bytes, returned as a slice.
    fn counted(&mut self, stride: usize) -> Option<&'a [u8]> {
        let count = self.u32()? as usize;
        let len = count.checked_mul(stride)?;
        let raw = self.data.get(self.pos..self.pos.checked_add(len)?)?;
        self.pos += len;
        Some(raw)
    }

    /// A counted block whose contents are deliberately not retained.
    fn skip_counted(&mut self, stride: usize) -> Option<()> {
        self.counted(stride).map(|_| ())
    }

    fn skip(&mut self, len: usize) -> Option<()> {
        self.pos = self.pos.checked_add(len)?;
        (self.pos <= self.data.len()).then_some(())
    }
}

/// Largest `divisor` this decoder will honour before declaring the stream
/// unrecognised. The segment grid is `divisor^2` counted lists, so a garbage
/// word here is the difference between a bounded walk and a very long one.
/// Shipped content stays far below: the observed maximum across `Skyrim.esm`
/// and `Dragonborn.esm` is single-digit.
const NVNM_MAX_DIVISOR: u32 = 64;

/// Decode a Creation-Engine `NVNM` blob into `out`'s canonical fields.
///
/// The header (worldspace + cell-or-grid) is decoded for **every** game that
/// ships the sub-record, because it reconciles everywhere it was measured —
/// including Fallout 4, whose interiors match the owning cell 781/781 and
/// whose exteriors match `XCLC` 7,113/7,113. The body is decoded only when
/// the whole stream reconciles to the blob length exactly; otherwise the
/// bytes are retained and the geometry fields stay empty.
///
/// See [`parse_navm`] for the layout and the evidence behind the branch.
fn decode_nvnm(data: &[u8], out: &mut NavmRecord) {
    let mut cur = NvnmCursor { data, pos: 0 };
    let header = (|| {
        cur.u32()?; // unknown — tracks NVER on the typed form
        cur.u32()?; // unknown — location-ish; not established by the corpus
        let worldspace = cur.u32()?;
        // Interior tiles name their cell; exterior tiles name their grid,
        // y before x. Both are four bytes, so this branch is invisible to a
        // length check — see `parse_navm` for how it was settled.
        let (cell_form, grid) = if worldspace == 0 {
            (Some(cur.u32()?), None)
        } else {
            let y = i32::from(cur.i16()?);
            let x = i32::from(cur.i16()?);
            (None, Some((x, y)))
        };
        Some((worldspace, cell_form, grid))
    })();

    let Some((worldspace, cell_form, grid)) = header else {
        // Not even the header reconciles — retain and report nothing.
        out.packed_geometry = Some(data.to_vec());
        return;
    };
    out.worldspace_form = Some(worldspace);
    if cell_form.is_some() {
        out.cell_form = cell_form;
    }
    out.grid = grid;

    let body = (|| {
        let vertices: Vec<[f32; 3]> = rows(cur.counted(12)?, 12)
            .map(|r| {
                [
                    f32::from_le_bytes([r[0], r[1], r[2], r[3]]),
                    f32::from_le_bytes([r[4], r[5], r[6], r[7]]),
                    f32::from_le_bytes([r[8], r[9], r[10], r[11]]),
                ]
            })
            .collect();
        // Byte-identical to the Gamebryo `NVTR` row, so the same decoder
        // serves both. Skyrim splits the trailing `u32` into a cover marker
        // and cover flags; carrying it whole keeps one canonical field.
        let triangles: Vec<NavmTriangle> =
            rows(cur.counted(16)?, 16).map(decode_nvtr_row).collect();
        // Likewise identical to `NVEX`.
        let external: Vec<NavmExternalConnection> = rows(cur.counted(10)?, 10)
            .filter_map(decode_nvex_row)
            .collect();
        // #3300 — captured rather than skipped. These go through the same
        // `counted()` walk that already reconciled 15,966/15,966 Skyrim
        // blobs, so the offsets are the proven ones; only the disposition
        // changed. The packed door row is 10 bytes to the typed `NVDP`'s 8 —
        // and its fields are in a *different order*: the triangle index leads,
        // then a 4-byte constant, then the door FormID. Established by
        // census over `Skyrim.esm`'s 1,703 rows — the leading `u16` is in
        // triangle range 1,703/1,703, the trailing `u32` takes 1,703 distinct
        // values in the plugin's own FormID range, and the middle `u32` takes
        // exactly **one** value (`0xE48B73F3`) across every row, so it is a
        // fixed marker rather than per-door data.
        let doors: Vec<NavmDoorTriangle> = rows(cur.counted(10)?, 10)
            .map(|r| NavmDoorTriangle {
                triangle: u16::from_le_bytes([r[0], r[1]]),
                unknown: u32::from_le_bytes([r[2], r[3], r[4], r[5]]),
                door_form_id: u32::from_le_bytes([r[6], r[7], r[8], r[9]]),
            })
            .collect();
        let covers: Vec<u16> = rows(cur.counted(2)?, 2)
            .map(|r| u16::from_le_bytes([r[0], r[1]]))
            .collect();
        let divisor = cur.u32()?;
        if divisor > NVNM_MAX_DIVISOR {
            return None;
        }
        cur.skip(4 * 8)?; // max X/Y distance + min/max XYZ bounds
        for _ in 0..(divisor * divisor) {
            cur.skip_counted(2)?; // per-segment triangle index list
        }
        (cur.pos == data.len()).then_some((vertices, triangles, external, doors, covers))
    })();

    match body {
        Some((vertices, triangles, external_connections, doors, covers)) => {
            out.vertices = vertices;
            out.triangles = triangles;
            out.external_connections = external_connections;
            out.door_triangles = doors;
            out.cover_triangles = covers;
        }
        // Header understood, body not (Fallout 4). Keep the bytes; the
        // association above is still usable for streaming.
        None => out.packed_geometry = Some(data.to_vec()),
    }
}

/// Decode one 16-byte triangle row, shared by `NVTR` and the `NVNM` body.
fn decode_nvtr_row(r: &[u8]) -> NavmTriangle {
    let w = |i: usize| u16::from_le_bytes([r[i * 2], r[i * 2 + 1]]);
    // 0xFFFF is the authored "no neighbour across this edge" sentinel — a
    // border triangle. Modelling it as `None` keeps a consumer from walking
    // into index 65535 and reading whatever sits there.
    let link = |i: usize| match w(i) {
        u16::MAX => None,
        other => Some(other),
    };
    NavmTriangle {
        vertices: [w(0), w(1), w(2)],
        edge_neighbours: [link(3), link(4), link(5)],
        flags: u32_at(r, 12).unwrap_or(0),
    }
}

/// Decode one 8-byte `NVDP` row: door `REFR` FormID, triangle index, then a
/// trailing `u16` whose meaning the corpus does not establish (see
/// [`NavmDoorTriangle::unknown`]).
fn decode_nvdp_row(r: &[u8]) -> NavmDoorTriangle {
    NavmDoorTriangle {
        door_form_id: u32::from_le_bytes([r[0], r[1], r[2], r[3]]),
        triangle: u16::from_le_bytes([r[4], r[5]]),
        unknown: u16::from_le_bytes([r[6], r[7]]) as u32,
    }
}

/// Decode an `NVGD` grid-acceleration payload.
///
/// Returns `None` unless the payload is consumed **exactly** — the same
/// all-or-nothing posture [`decode_nvnm`] takes, so a format revision surfaces
/// as "not recognised" rather than as a half-filled lattice. Shipped data
/// reconciles 11,969/11,969.
fn decode_nvgd(data: &[u8]) -> Option<NavmGridAccel> {
    const HEADER: usize = 4 + 4 * 8;
    if data.len() < HEADER {
        return None;
    }
    let divisor = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    // Same bound as the packed form's lattice: the cell count is `divisor^2`,
    // so a garbage word here is the difference between a bounded walk and a
    // very long one. Shipped maximum is 12.
    if divisor == 0 || divisor > NVNM_MAX_DIVISOR {
        return None;
    }
    let mut bounds = [0.0f32; 8];
    for (i, slot) in bounds.iter_mut().enumerate() {
        let o = 4 + i * 4;
        *slot = f32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
    }
    let cell_count = (divisor as usize).checked_mul(divisor as usize)?;
    let mut cells = Vec::with_capacity(cell_count);
    let mut pos = HEADER;
    for _ in 0..cell_count {
        let count = u16::from_le_bytes([*data.get(pos)?, *data.get(pos + 1)?]) as usize;
        pos += 2;
        let end = pos.checked_add(count.checked_mul(2)?)?;
        let raw = data.get(pos..end)?;
        cells.push(
            raw.chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect(),
        );
        pos = end;
    }
    (pos == data.len()).then_some(NavmGridAccel {
        divisor,
        bounds,
        cells,
    })
}

/// Decode one 10-byte external-connection row, shared by `NVEX` and `NVNM`.
fn decode_nvex_row(r: &[u8]) -> Option<NavmExternalConnection> {
    Some(NavmExternalConnection {
        unknown: u32_at(r, 0)?,
        mesh_form: u32_at(r, 4)?,
        triangle: u16::from_le_bytes([r[8], r[9]]),
    })
}

/// What a [`RegionDataEntry`] configures. The discriminants are the raw
/// `RDAT` type word.
///
/// Derived by correlating every `RDAT` type against the sub-records that
/// follow it across `Oblivion.esm` (211 regions), `FalloutNV.esm` (276) and
/// `Skyrim.esm` (317) — see [`parse_regn`] for the full table. Not every game
/// authors every type: `Grass` appears once in Oblivion and nowhere else,
/// `Imposter` is FNV-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionDataKind {
    /// `RDOT` — scattered objects (rocks, flora) placed procedurally.
    Objects,
    /// `RDWT` — the region's weather table.
    Weather,
    /// `RDMP` — map name shown for this region.
    Map,
    /// `ICON` — landscape texture association.
    Landscape,
    /// `RDGS` — grass placement.
    Grass,
    /// Ambient audio: `RDSD`/`RDSA` sound list plus `RDMD`/`RDMO`/`RDSB`
    /// music or blanket-sound form.
    Sound,
    /// `RDID` — imposters (FNV).
    Imposter,
    /// A type this parser does not model. Carries the raw word so an
    /// unexpected value is reported rather than silently dropped.
    Unknown(u32),
}

impl RegionDataKind {
    fn from_word(word: u32) -> Self {
        match word {
            2 => Self::Objects,
            3 => Self::Weather,
            4 => Self::Map,
            5 => Self::Landscape,
            6 => Self::Grass,
            7 => Self::Sound,
            8 => Self::Imposter,
            other => Self::Unknown(other),
        }
    }
}

/// One weather-table row: a `WTHR` form and its selection chance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegionWeather {
    pub weather_form: u32,
    /// Percent chance, 0–100. Observed values are 40 / 50 / 85 / 100.
    pub chance: u32,
    /// `GLOB` form gating this row. `None` on Oblivion, whose 8-byte row has
    /// no such field.
    pub global_form: Option<u32>,
}

/// One ambient-sound row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegionSound {
    pub sound_form: u32,
    /// Playback flags (pleasant / cloudy / rainy / snowy bits).
    pub flags: u32,
    /// Raw chance word. Deliberately **not** rescaled: every observed value is
    /// a multiple of 10,000 (200000, 400000, 700000, 2000000, 2500000), which
    /// is consistent with a fixed-point percentage but the divisor is not
    /// established by the data. Left raw rather than guessed at; a consumer
    /// that needs a probability should pin the scale against the game first.
    pub chance_raw: u32,
}

/// One `RDAT` entry: what it configures, how it ranks, and its payload.
#[derive(Debug, Clone, PartialEq)]
pub struct RegionDataEntry {
    pub kind: RegionDataKind,
    /// `RDAT` flags byte. Only bit 0 is ever set in shipped data.
    pub flags: u8,
    /// `RDAT` priority byte, 0–100 (50 is overwhelmingly the default).
    ///
    /// This is the ordering EX-16 requires to be deterministic: it is
    /// *authored*, not invented by the engine, so a consumer must sort by it
    /// rather than by record order.
    pub priority: u8,
    pub payload: RegionDataPayload,
}

/// Type-specific payload of a [`RegionDataEntry`].
#[derive(Debug, Clone, PartialEq)]
pub enum RegionDataPayload {
    /// `RDOT` object FormIDs. The 52-byte row carries density/clustering
    /// fields beyond the form; only the form is decoded, since the remaining
    /// layout is not established by the corpus.
    Objects(Vec<u32>),
    Weather(Vec<RegionWeather>),
    /// `RDMP` map name as inline text (Oblivion / FO3 / FNV).
    Map(String),
    /// `RDMP` map name as a localised string ID (Skyrim+), resolved against
    /// the plugin's `.STRINGS` tables rather than stored inline.
    MapStringId(u32),
    /// `ICON` landscape-texture path.
    Landscape(String),
    /// `RDGS` raw payload — a single 32-byte sample exists (one Oblivion
    /// region), too little to establish a row layout.
    Grass(Vec<u8>),
    Sound {
        /// `RDMD` (Oblivion) / `RDMO` (Skyrim) / `RDSB` (FNV) — the raw
        /// 4-byte value, decoded generically as a FormID.
        ///
        /// #3787 — on FNV this is **confirmed NOT a `SOUN` FormID**: a
        /// census across all 276 `FalloutNV.esm` REGN records found 44 of
        /// 44 `RDSB` targets resolve as `MSET` (Media Set), 0 as `SOUN`.
        /// `dispatch_region_ambient_music` (`byroredux/src/asset_provider/
        /// audio.rs`) resolves `music_form` against the parsed `SounRecord`
        /// map regardless, so it misses on every FNV region — the consumer
        /// treats this field as if it always names a `SOUN`, which was
        /// never true for FNV and is empirically **also** not true for
        /// Oblivion or Skyrim (their `RDMD`/`RDMO` don't resolve as `SOUN`
        /// either — Oblivion's values were measured near-universally `0`
        /// across every vanilla region, consistent with a small enum
        /// rather than a FormID at all; Skyrim's are real non-trivial
        /// FormIDs that still don't match any parsed `SOUN` record). The
        /// per-era target type this field actually names for Oblivion/
        /// Skyrim was NOT further verified here — flagged as a follow-up,
        /// not fixed. Do not assume `SOUN` for any era without checking.
        music: Option<u32>,
        /// `RDSI` incidental sound form (FNV). Same caveat as `music`
        /// above: census-confirmed 10 of 11 `RDSI` targets are `MSET`, not
        /// `SOUN` (#3787).
        incidental: Option<u32>,
        sounds: Vec<RegionSound>,
    },
    /// `RDID` imposter FormIDs (FNV).
    Imposters(Vec<u32>),
    /// The entry declared a type but authored no following payload.
    Empty,
}

/// A closed polygon bounding part of a region, with its edge falloff.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RegionArea {
    /// `RPLI` — edge fall-off distance in world units.
    pub edge_fall_off: u32,
    /// `RPLD` — polygon vertices in world XY. Stride is 8 bytes
    /// (two `f32`), confirmed across all three corpora.
    pub points: Vec<(f32, f32)>,
}

/// Region record (`REGN`). Tags a world-space area with a weather type, a
/// colour tint, one or more bounding polygons, and a priority-ordered chain of
/// `RDAT` data entries driving ambient sound, weather, objects and map name.
#[derive(Debug, Clone, Default)]
pub struct RegnRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `WNAM` — weather form that this region enforces. `None` when the
    /// region inherits from its worldspace.
    pub weather_form: Option<u32>,
    /// `RCLR` — RGB region tint for map shading. Stored as raw u8[3];
    /// alpha byte (if any) is ignored.
    pub color: Option<[u8; 3]>,
    /// `RPLI`/`RPLD` bounding polygons. A region may declare several.
    pub areas: Vec<RegionArea>,
    /// `RDAT` chain in authored order. Sort by
    /// [`priority`](RegionDataEntry::priority) for evaluation.
    pub entries: Vec<RegionDataEntry>,
}

impl RegnRecord {
    /// Entries of one kind, highest priority first.
    ///
    /// Ties keep authored order (the sort is stable), which is the only
    /// defensible tie-break: nothing in the record distinguishes two entries
    /// at equal priority.
    pub fn entries_by_priority(&self, kind: RegionDataKind) -> Vec<&RegionDataEntry> {
        let mut matching: Vec<&RegionDataEntry> =
            self.entries.iter().filter(|e| e.kind == kind).collect();
        matching.sort_by_key(|entry| std::cmp::Reverse(entry.priority));
        matching
    }
}

/// Resolve the highest-priority `RDAT` `Sound` entry across every region
/// tagging one resident cell (`CellData::regions`, XCLR) — EX-16 item 1's
/// "REGN runtime consumption" (#2372).
///
/// A cell can be tagged by several overlapping `REGN` polygons at once;
/// `RDAT`'s authored `priority` byte is the only cross-region ordering
/// signal that exists (there is no "region A outranks region B" field).
/// Ties keep authoring order — `region_form_ids` order first, then
/// within-region entry order — the same tie-break
/// [`RegnRecord::entries_by_priority`] uses, generalised across more than
/// one region.
///
/// A FormID absent from `regions` (bad data, or a REGN this parser never
/// saw) is skipped rather than treated as an error — mirrors every other
/// "resolve a FormID, tolerate a miss" site in this crate.
pub fn select_active_region_sound<'a>(
    region_form_ids: &[u32],
    regions: &'a HashMap<u32, RegnRecord>,
) -> Option<&'a RegionDataEntry> {
    let mut candidates: Vec<&RegionDataEntry> = region_form_ids
        .iter()
        .filter_map(|id| regions.get(id))
        .flat_map(|r| r.entries.iter())
        .filter(|e| e.kind == RegionDataKind::Sound)
        .collect();
    candidates.sort_by_key(|entry| std::cmp::Reverse(entry.priority));
    candidates.into_iter().next()
}

/// Decode a little-endian `u32` at `offset`, or `None` past the end.
fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let slice = data.get(offset..end)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// Split `data` into fixed-size rows, ignoring a short trailing remainder.
///
/// Every `RDAT` payload observed in the corpus is an exact multiple of its row
/// size; truncating rather than erroring keeps one malformed plugin from
/// dropping an entire region.
fn rows(data: &[u8], stride: usize) -> impl Iterator<Item = &[u8]> {
    data.chunks_exact(stride.max(1))
}

/// Strict sibling of [`rows`]: refuses a non-zero remainder instead of
/// silently discarding it. Mirrors the all-or-nothing posture
/// [`decode_nvgd`] and [`decode_nvnm`] already take — a `None` here should
/// make the caller treat the whole sub-record as unrecognised (an
/// empty/default field) rather than half-filled, so a stride that stops
/// matching a future game's data surfaces immediately instead of degrading
/// silently (#3404).
fn rows_exact(data: &[u8], stride: usize) -> Option<impl Iterator<Item = &[u8]>> {
    if stride == 0 || !data.len().is_multiple_of(stride) {
        return None;
    }
    Some(data.chunks_exact(stride))
}

/// Parse a region (`REGN`), including the `RDAT` data-entry chain (#2737).
///
/// # Format, as established from shipped data
///
/// `RDAT` is always 8 bytes: `type: u32, flags: u8, priority: u8, unused: u16`.
/// The trailing half-word is zero in every one of the 788 entries across
/// `Oblivion.esm`, `FalloutNV.esm` and `Skyrim.esm`; `flags` is only ever 0 or
/// 1; `priority` ranges 0–100 with 50 overwhelmingly dominant.
///
/// Each `RDAT` opens a section, and the sub-records *following* it until the
/// next `RDAT` are its payload. The type → payload mapping was derived by
/// tabulating which signatures follow which type word:
///
/// | type | payload | Oblivion | FNV | Skyrim |
/// |---|---|---|---|---|
/// | 2 | `RDOT` objects | 71 | 19 | 69 (all empty) |
/// | 3 | `RDWT` weather | 57 | 31 | 53 |
/// | 4 | `RDMP` map name | 113 | 55 | 7 |
/// | 5 | `ICON` landscape | 1 | 9 | 1 |
/// | 6 | `RDGS` grass | 1 | — | — |
/// | 7 | sound | `RDMD`+`RDSD` | `RDSB`+`RDSD`+`RDSI` | `RDMO`+`RDSA` |
/// | 8 | `RDID` imposters | — | 18 | — |
///
/// Row strides were established the same way, by taking the GCD of every
/// observed payload length and confirming the smallest observed length equals
/// it. **`RDWT` is the one genuine per-game divergence**: Oblivion's row is 8
/// bytes (`weather`, `chance`) while FO3/FNV/Skyrim's is 12 (`weather`,
/// `chance`, `global`). Both are handled by measuring the payload rather than
/// branching on a game enum — a payload divisible by 12 but not 8 can only be
/// the long form, and the short form is assumed otherwise.
pub fn parse_regn(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> RegnRecord {
    let mut out = RegnRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs(subs);
    out.editor_id = common.editor_id;

    // Section state: the RDAT currently open, and the area currently being
    // accumulated. Both flush when their next opener arrives or at the end.
    let mut open: Option<RegionDataEntry> = None;
    let mut area: Option<RegionArea> = None;

    for sub in subs {
        match &sub.sub_type {
            b"WNAM" if sub.data.len() >= 4 => {
                out.weather_form = SubReader::new(&sub.data).u32().ok();
            }
            b"RCLR" if sub.data.len() >= 3 => {
                out.color = Some([sub.data[0], sub.data[1], sub.data[2]]);
            }
            b"RPLI" => {
                if let Some(previous) = area.take() {
                    out.areas.push(previous);
                }
                area = Some(RegionArea {
                    edge_fall_off: u32_at(&sub.data, 0).unwrap_or(0),
                    points: Vec::new(),
                });
            }
            b"RPLD" => {
                // Stride 8 = two f32. An RPLD without a preceding RPLI still
                // describes a real polygon, so synthesise a zero-falloff area
                // rather than dropping the geometry.
                let target = area.get_or_insert_with(RegionArea::default);
                for row in rows(&sub.data, 8) {
                    let x = f32::from_le_bytes([row[0], row[1], row[2], row[3]]);
                    let y = f32::from_le_bytes([row[4], row[5], row[6], row[7]]);
                    target.points.push((x, y));
                }
            }
            b"RDAT" if sub.data.len() >= 8 => {
                if let Some(previous) = open.take() {
                    out.entries.push(previous);
                }
                open = Some(RegionDataEntry {
                    kind: RegionDataKind::from_word(u32_at(&sub.data, 0).unwrap_or(0)),
                    flags: sub.data[4],
                    priority: sub.data[5],
                    payload: RegionDataPayload::Empty,
                });
            }
            other => {
                let Some(entry) = open.as_mut() else { continue };
                apply_region_payload(entry, other, &sub.data);
            }
        }
    }
    if let Some(previous) = open.take() {
        out.entries.push(previous);
    }
    if let Some(previous) = area.take() {
        out.areas.push(previous);
    }
    // #3401 — one post-pass over every FormID-bearing field, for the same
    // reason `parse_navm` uses one: the `RDAT` payloads are built across
    // several sub-records by `apply_region_payload`, so remapping at the
    // decode sites would mean touching every arm of a growing match.
    out.weather_form = out.weather_form.map(|f| remap_fid(f, remap));
    for entry in &mut out.entries {
        match &mut entry.payload {
            RegionDataPayload::Objects(forms) | RegionDataPayload::Imposters(forms) => {
                for f in forms {
                    *f = remap_fid(*f, remap);
                }
            }
            RegionDataPayload::Weather(rows) => {
                for row in rows {
                    row.weather_form = remap_fid(row.weather_form, remap);
                    row.global_form = row.global_form.map(|f| remap_fid(f, remap));
                }
            }
            RegionDataPayload::Sound {
                music,
                incidental,
                sounds,
            } => {
                *music = music.map(|f| remap_fid(f, remap));
                *incidental = incidental.map(|f| remap_fid(f, remap));
                for snd in sounds {
                    snd.sound_form = remap_fid(snd.sound_form, remap);
                }
            }
            RegionDataPayload::Map(_)
            | RegionDataPayload::MapStringId(_)
            | RegionDataPayload::Landscape(_)
            | RegionDataPayload::Grass(_)
            | RegionDataPayload::Empty => {}
        }
    }
    out
}

/// Decode an `RDWT` weather table, choosing the row stride by validation.
///
/// Oblivion's row is 8 bytes (`weather`, `chance`); FO3/FNV/Skyrim's is 12
/// (`weather`, `chance`, `global`). Lengths divisible by both — 24, 48 — are
/// genuinely ambiguous, and Oblivion authors plenty of them, so a
/// divisibility tie-break silently mis-splits real data. (It did: an early
/// version preferring 12 produced 65 Oblivion rows with `chance > 100`.)
///
/// `chance` is a percentage, so it is its own checksum. Decode with each
/// candidate stride and keep the one whose chances are all in range; prefer
/// the 12-byte form only when it validates and the 8-byte form does not, so
/// the common unambiguous cases stay exact.
fn decode_weather_rows(data: &[u8]) -> Vec<RegionWeather> {
    fn decode(data: &[u8], stride: usize, exact: bool) -> Option<Vec<RegionWeather>> {
        if exact && !data.len().is_multiple_of(stride) {
            return None;
        }
        let out: Vec<RegionWeather> = rows(data, stride)
            .filter_map(|r| {
                Some(RegionWeather {
                    weather_form: u32_at(r, 0)?,
                    chance: u32_at(r, 4)?,
                    global_form: if stride >= 12 { u32_at(r, 8) } else { None },
                })
            })
            .collect();
        // An empty result must not count as "validated" — `all()` is
        // vacuously true on an empty iterator, which would let a stride that
        // yielded nothing win over one that yielded good rows.
        (!out.is_empty() && out.iter().all(|w| w.chance <= 100)).then_some(out)
    }
    // Exact divisibility first, and 8 before 12. Both orderings matter:
    //
    // - Exact-first is what stops a 12-byte long row being read as a single
    //   8-byte row. Its `chance` sits at the same offset in both readings, so
    //   validation alone cannot tell them apart — only the length can.
    // - 8-before-12 resolves the genuinely ambiguous lengths (24, 48) in
    //   Oblivion's favour, and validation catches the case where that is
    //   wrong: reading FNV's 2x12 payload as 3x8 puts a FormID in the second
    //   row's `chance`, which fails the <=100 check and falls through to 12.
    //
    // The lenient pass is a last resort for a ragged payload, so one
    // malformed plugin loses a row rather than the whole table.
    decode(data, 8, true)
        .or_else(|| decode(data, 12, true))
        .or_else(|| decode(data, 8, false))
        .or_else(|| decode(data, 12, false))
        .unwrap_or_default()
}

/// Fold one payload sub-record into the currently-open `RDAT` section.
fn apply_region_payload(entry: &mut RegionDataEntry, sig: &[u8], data: &[u8]) {
    match sig {
        b"RDOT" => {
            let forms: Vec<u32> = rows(data, 52).filter_map(|r| u32_at(r, 0)).collect();
            entry.payload = RegionDataPayload::Objects(forms);
        }
        b"RDWT" => {
            entry.payload = RegionDataPayload::Weather(decode_weather_rows(data));
        }
        b"RDMP" => {
            // Skyrim localises RDMP: the payload is a 4-byte string ID into
            // the .STRINGS tables rather than inline text, while Oblivion /
            // FO3 / FNV author a zero-terminated string. Reading the localised
            // form as text yields control characters, which is exactly what a
            // corpus sweep showed before this branch existed.
            if data.len() == 4 {
                entry.payload =
                    RegionDataPayload::MapStringId(u32_at(data, 0).expect("length checked"));
            } else {
                entry.payload = RegionDataPayload::Map(read_zstring(data));
            }
        }
        b"ICON" => entry.payload = RegionDataPayload::Landscape(read_zstring(data)),
        b"RDGS" => entry.payload = RegionDataPayload::Grass(data.to_vec()),
        b"RDID" => {
            let forms: Vec<u32> = rows(data, 4).filter_map(|r| u32_at(r, 0)).collect();
            entry.payload = RegionDataPayload::Imposters(forms);
        }
        // Sound sections mix a single music/blanket form with a variable
        // sound list, and the signatures differ per game, so the payload is
        // built up across several sub-records rather than replaced.
        b"RDSD" | b"RDSA" => {
            let list: Vec<RegionSound> = rows(data, 12)
                .filter_map(|r| {
                    Some(RegionSound {
                        sound_form: u32_at(r, 0)?,
                        flags: u32_at(r, 4)?,
                        chance_raw: u32_at(r, 8)?,
                    })
                })
                .collect();
            merge_sound(entry, |p| {
                if let RegionDataPayload::Sound { sounds, .. } = p {
                    *sounds = list;
                }
            });
        }
        b"RDMD" | b"RDMO" | b"RDSB" => {
            let value = u32_at(data, 0);
            merge_sound(entry, |p| {
                if let RegionDataPayload::Sound { music, .. } = p {
                    *music = value;
                }
            });
        }
        b"RDSI" => {
            let value = u32_at(data, 0);
            merge_sound(entry, |p| {
                if let RegionDataPayload::Sound { incidental, .. } = p {
                    *incidental = value;
                }
            });
        }
        _ => {}
    }
}

/// Ensure `entry` holds a `Sound` payload, then apply `f` to it.
fn merge_sound(entry: &mut RegionDataEntry, f: impl FnOnce(&mut RegionDataPayload)) {
    if !matches!(entry.payload, RegionDataPayload::Sound { .. }) {
        entry.payload = RegionDataPayload::Sound {
            music: None,
            incidental: None,
            sounds: Vec::new(),
        };
    }
    f(&mut entry.payload);
}

/// Encounter zone (`ECZN`). Governs spawn scaling / faction ownership
/// on the cells that reference it via `XEZN`. The `DATA` layout is:
/// `owner (u32 FormID) + rank (u8) + min-level (u8) + flags (u8) + unused (u8)`.
#[derive(Debug, Clone, Default)]
pub struct EcznRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Form ID of the faction or actor that owns this zone. `0` when
    /// the field is unset (wilderness zones sometimes leave it blank).
    pub owner_form: u32,
    /// Faction rank required; 0 = no rank gate.
    pub rank: u8,
    /// Minimum player level for zone to unlock spawn overrides.
    pub min_level: u8,
    pub flags: u8,
}

pub fn parse_eczn(form_id: u32, subs: &[SubRecord]) -> EcznRecord {
    let mut out = EcznRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs(subs);
    out.editor_id = common.editor_id;
    for sub in subs {
        match &sub.sub_type {
            b"DATA" if sub.data.len() >= 7 => {
                let mut r = SubReader::new(&sub.data);
                out.owner_form = r.u32_or_default();
                out.rank = r.u8_or_default();
                out.min_level = r.u8_or_default();
                out.flags = r.u8_or_default();
            }
            _ => {}
        }
    }
    out
}

/// Lighting template (`LGTM`). Provides a named bundle of XCLL-shaped
/// lighting values that cells can reference via `LTMP` and selectively
/// override per-field. Full per-field inheritance fallback lands
/// alongside #379. Skyrim templates also carry the extended fog/fade
/// values in `DATA` and the directional ambient/specular bundle in
/// `DALC`; both are needed when CELL.XCLL selects template fields via
/// its inheritance mask.
///
/// The `DATA` sub-record mirrors XCLL byte-for-byte (bytes 0-39):
///   0-3:   ambient  (RGBA, byte order per cell.rs XCLL parser)
///   4-7:   directional (RGBA)
///   8-11:  fog color (RGBA)
///   12-15: fog near (f32)
///   16-19: fog far (f32)
///   20-23: rotation X (i32 degrees)
///   24-27: rotation Y (i32 degrees)
///   28-31: directional fade (f32)
///   32-35: fog clip (f32)
///   36-39: fog power (f32)
#[derive(Debug, Clone, Default)]
pub struct LgtmRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Ambient color normalised to [0, 1] RGB. XCLL uses RGB byte order
    /// (see `cell.rs` comment; corrected post-#389 revert).
    pub ambient: [f32; 3],
    pub directional: [f32; 3],
    pub fog_color: [f32; 3],
    pub fog_near: f32,
    pub fog_far: f32,
    pub directional_rotation: [f32; 2],
    pub directional_fade: Option<f32>,
    pub fog_clip: Option<f32>,
    pub fog_power: Option<f32>,
    pub fog_far_color: Option<[f32; 3]>,
    pub fog_max: Option<f32>,
    pub light_fade_begin: Option<f32>,
    pub light_fade_end: Option<f32>,
    pub directional_ambient: Option<[[f32; 3]; 6]>,
    pub specular_color: Option<[f32; 3]>,
    pub specular_alpha: Option<f32>,
    pub fresnel_power: Option<f32>,
}

pub fn parse_lgtm(form_id: u32, subs: &[SubRecord]) -> LgtmRecord {
    let mut out = LgtmRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs(subs);
    out.editor_id = common.editor_id;
    for sub in subs {
        match &sub.sub_type {
            b"DATA" if sub.data.len() >= 20 => {
                let mut r = SubReader::new(&sub.data);
                out.ambient = r.rgb_color().unwrap_or([0.0; 3]);
                out.directional = r.rgb_color().unwrap_or([0.0; 3]);
                out.fog_color = r.rgb_color().unwrap_or([0.0; 3]);
                out.fog_near = r.f32_or_default();
                out.fog_far = r.f32_or_default();
                let rot_x = (r.i32_or_default() as f32).to_radians();
                let rot_y = (r.i32_or_default() as f32).to_radians();
                out.directional_rotation = [rot_x, rot_y];
                out.directional_fade = r.f32().ok();
                out.fog_clip = r.f32().ok();
                out.fog_power = r.f32().ok();

                // Skyrim v34+ DATA mirrors XCLL except that its ambient
                // bundle at 40-71 is explicitly unused; the live ambient
                // cube/specular/fresnel values are in DALC below. The final
                // u32 (88-91) is likewise unused on LGTM.
                if sub.data.len() >= 92 {
                    r.skip_or_eof(32);
                    out.fog_far_color = r.rgb_color().ok();
                    out.fog_max = r.f32().ok();
                    out.light_fade_begin = r.f32().ok();
                    out.light_fade_end = r.f32().ok();
                }
            }
            b"DALC" if sub.data.len() >= 24 => {
                let mut r = SubReader::new(&sub.data);
                let mut ambient_cube = [[0.0f32; 3]; 6];
                for face in &mut ambient_cube {
                    *face = r.rgb_color().unwrap_or([0.0; 3]);
                }
                out.directional_ambient = Some(ambient_cube);
                if let Ok(spec) = r.rgba_color() {
                    out.specular_color = Some([spec[0], spec[1], spec[2]]);
                    out.specular_alpha = Some(spec[3]);
                }
                out.fresnel_power = r.f32().ok();
            }
            _ => {}
        }
    }
    out
}

/// Image-space record (`IMGS`). Drives per-cell HDR / colour-grading
/// settings — cells reference an IMGS via `XCIM` to override the
/// worldspace-default tone-map / cinematic / tint LUT.
///
/// Skyrim ships ~1k IMGS entries; almost every Solitude / Whiterun
/// interior overrides the worldspace default. Vanilla Skyrim's
/// `DNAM` is 152 bytes (HDR eye-adapt + cinematic
/// saturation/brightness/contrast + tint RGBA + bloom params);
/// FO3/FNV's is the 56-byte subset. Pre-#624 the entire top-level
/// `IMGS` group fell through to the catch-all skip in `parse_esm`,
/// so XCIM cross-references couldn't resolve to anything in the
/// index.
///
/// This stub captures `EDID` + the raw `DNAM` payload so a future
/// per-cell HDR-LUT consumer can decode the tone-map fields lazily
/// without re-walking the ESM. The full DNAM struct decode + IMAD
/// modifier-graph parser are deferred to M48.
#[derive(Debug, Clone, Default)]
pub struct ImgsRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Raw `DNAM` payload — Skyrim 152 B (HDR + cinematic + tint),
    /// FO3/FNV 56 B (subset). `None` when the record has no DNAM
    /// (rare; a few legacy entries on FO3/FNV).
    pub dnam_raw: Option<Vec<u8>>,
}

/// Parse an `IMGS` record into an [`ImgsRecord`]. Mirrors the
/// stub-shape of [`parse_lgtm`] — captures EDID + the data payload
/// and defers field-by-field decoding to the consumer. See #624 /
/// SK-D6-NEW-03.
pub fn parse_imgs(form_id: u32, subs: &[SubRecord]) -> ImgsRecord {
    let mut out = ImgsRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs(subs);
    out.editor_id = common.editor_id;
    for sub in subs {
        if &sub.sub_type == b"DNAM" {
            out.dnam_raw = Some(sub.data.clone());
        }
    }
    out
}

/// `ACTI` activator record. FO3/FNV/Oblivion wall switches, buttons,
/// vending machines, lever-activated doors — anything that the player
/// "use"s but isn't a container, door, or NPC. SCRI on these records
/// runs the trigger script; DEST controls destruction-state meshes.
/// Full destruction-stage decoding is deferred — the stub captures
/// identity + model + SCRI cross-ref so dangling references resolve
/// at lookup time. See #521.
///
/// **`script_form_id` is live since M47.0**: `ActiRecord` is the first arm
/// of `EsmIndex::base_record_script` (`records/index.rs`), which
/// `byroredux/src/cell_loader/references/attach.rs` calls to resolve
/// `index.scripts.get(&script_form_id)` and dispatch `ActivateEvent` to the
/// SCRI-linked script (the attach path logs with an `"M47.0: "` prefix).
///
/// **Runtime consumer gap, still open:** `sound_form_id` / `radio_form_id`
/// cross-refs ride through unused today — no reader outside `records/`
/// plays the SNAM/RNAM sound on `OnActivate` yet. The stub closes the
/// parser-side silent drop so that work has a grep target.
#[derive(Debug, Clone, Default)]
pub struct ActiRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// NIF path from MODL — already populated in `cells.statics` via
    /// the MODL catch-all, but duplicated here so a structured record
    /// map is internally consistent.
    pub model_path: String,
    /// SCRI — script form ID attached to this activator. `0` = no
    /// script. Live since M47.0 via `EsmIndex::base_record_script` +
    /// `cell_loader::references::attach`, which dispatches `ActivateEvent`
    /// to the resolved script.
    pub script_form_id: u32,
    /// SNAM — sound form ID played on activation (optional).
    pub sound_form_id: u32,
    /// RADR / RNAM — radio station form ID, applicable to FNV radio
    /// transmitters (activator variant). `0` when absent.
    pub radio_form_id: u32,
    /// Decoded `VMAD` script attachments (Skyrim+ inline Papyrus). `None`
    /// on FO3 / FNV / Oblivion activators (those use the `SCRI` →  SCPT /
    /// Obscript path). Consumed by the M47.2 scripting-translation layer
    /// to fetch + decompile the attached `.pex`. Activators are the most
    /// common scripted base record in Skyrim cells (levers, doors, traps).
    pub script_instance: Option<crate::esm::records::script_instance::ScriptInstanceData>,
}

pub fn parse_acti(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> ActiRecord {
    // EDID / FULL / MODL / SCRI / VMAD via the shared helper (TD2-109 /
    // #2068). VMAD is the Skyrim+ inline Papyrus attachment — absent on
    // FO3/FNV, which carry SCRI instead — and the helper decodes it so the
    // M47.2 attach path can decompile the named `.pex` and bind its
    // properties. Only the ACTI-specific sound / radio arms remain below.
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    let mut out = ActiRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        model_path: common.model_path,
        script_form_id: common.script_form_id,
        script_instance: common.script_instance,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            // #3401 — `parse_acti` already receives the remap and applies
            // it to the shared named fields; these two read FormIDs a
            // dozen lines later and used to skip it.
            b"SNAM" => {
                out.sound_form_id = remap_fid(SubReader::new(&sub.data).u32_or_default(), remap);
            }
            b"RNAM" | b"RADR" => {
                out.radio_form_id = remap_fid(SubReader::new(&sub.data).u32_or_default(), remap);
            }
            _ => {}
        }
    }
    out
}

/// `TERM` terminal record — FO3/FNV computer consoles. Carries a
/// menu tree (MNAM entries), password (ANAM), body text (DNAM), and
/// the NIF model of the physical terminal. MNAM text is collected
/// into `menu_items` so a future terminal-interaction system can
/// walk the options without re-parsing. See #521.
///
/// **SCRI is live since M47.0**: `TermRecord` is one of the
/// `EsmIndex::base_record_script` arms (`records/index.rs`), so
/// `cell_loader::references::attach` resolves and dispatches a hacked
/// terminal's script the same generic way it does for `ACTI`.
///
/// **Runtime consumer gap, still open:** the menu tree and password ride
/// through unused — terminal interaction needs the event-hook runtime for
/// NNAM target dispatch + CTDA option-gate evaluation, plus a UI overlay
/// for the `body_size`-driven screen. The stub captures the surface so
/// that work has one grep target and the labels don't have to be
/// re-walked from the ESM.
#[derive(Debug, Clone, Default)]
pub struct TermRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub model_path: String,
    /// SCRI — script form ID (some terminals trigger quest advance
    /// scripts on successful hack). Live since M47.0 via
    /// `EsmIndex::base_record_script` + `cell_loader::references::attach`.
    pub script_form_id: u32,
    /// ANAM — password string (may be empty for unlocked terminals).
    pub password: String,
    /// DNAM — footer / body text displayed on the terminal screen.
    pub footer_text: String,
    /// BSIZ — body-size scalar (u8, 0 = small, 1 = large). Defaults 0.
    pub body_size: u8,
    /// MNAM — menu-item text, one per entry. Order preserved. Each
    /// MNAM is flanked by its own sub-record group (NNAM target,
    /// CTDA conditions) which is deferred; the stub just captures
    /// the labels so the menu tree isn't lost.
    pub menu_items: Vec<String>,
    /// Decoded `VMAD` script attachments (Skyrim+ inline Papyrus). `None`
    /// on FO3/FNV terminals (those use the `SCRI` → SCPT/Obscript path
    /// via `script_form_id`). #2663 (SCR-D7-NEW11-02) — FO4 ships 207
    /// VMAD-bearing `TERM` records (`Fallout4.esm`); the shared helper
    /// already decoded this, `parse_term` just wasn't copying it out.
    pub script_instance: Option<super::super::script_instance::ScriptInstanceData>,
}

pub fn parse_term(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> TermRecord {
    // EDID / FULL / MODL / SCRI / VMAD via the shared helper (TD2-109 /
    // #2068). TERM is NOT FO3/FNV-only — FO4 ships 207 VMAD-bearing TERM
    // records — so the helper's VMAD arm fires there; #2663 fixed
    // `parse_term` dropping the decoded `script_instance` on the floor.
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    let mut out = TermRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        model_path: common.model_path,
        script_form_id: common.script_form_id,
        script_instance: common.script_instance,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"ANAM" => out.password = read_zstring(&sub.data),
            b"DNAM" => out.footer_text = read_zstring(&sub.data),
            b"BSIZ" if !sub.data.is_empty() => {
                out.body_size = sub.data[0];
            }
            b"MNAM" => {
                // FO3/FNV sometimes ships MNAM as the menu-item text
                // directly and sometimes as a 4-byte form ref (when
                // the label lives elsewhere). Treat as text whenever
                // the bytes are printable; otherwise skip. Keeps the
                // stub robust against the mixed wild encoding.
                let text = read_zstring(&sub.data);
                if !text.is_empty() {
                    out.menu_items.push(text);
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(typ: &[u8; 4], data: &[u8]) -> SubRecord {
        SubRecord {
            sub_type: *typ,
            data: data.to_vec(),
        }
    }

    #[test]
    fn parse_navi_extracts_version() {
        let subs = vec![
            sub(b"EDID", b"NavMaster\0"),
            sub(b"NVER", &11u32.to_le_bytes()),
        ];
        let n = parse_navi(0x5678, &subs);
        assert_eq!(n.editor_id, "NavMaster");
        assert_eq!(n.version, 11);
    }

    #[test]
    fn parse_navm_extracts_version() {
        let subs = vec![sub(b"NVER", &11u32.to_le_bytes())];
        let n = parse_navm(0xAABB, &subs, &None);
        assert_eq!(n.form_id, 0xAABB);
        assert_eq!(n.version, 11);
    }

    #[test]
    fn parse_regn_picks_weather_and_color() {
        let subs = vec![
            sub(b"EDID", b"WastelandRegion\0"),
            sub(b"WNAM", &0x0001_B000u32.to_le_bytes()),
            sub(b"RCLR", &[128, 96, 64, 0]),
        ];
        let r = parse_regn(0xBEEF, &subs, &None);
        assert_eq!(r.editor_id, "WastelandRegion");
        assert_eq!(r.weather_form, Some(0x0001_B000));
        assert_eq!(r.color, Some([128, 96, 64]));
    }

    #[test]
    fn parse_eczn_picks_owner_rank_flags() {
        let mut data = Vec::new();
        data.extend_from_slice(&0x0001_CAFEu32.to_le_bytes()); // owner form
        data.push(3); // rank
        data.push(15); // min level
        data.push(0x02); // flags
        let subs = vec![sub(b"EDID", b"NcrZone\0"), sub(b"DATA", &data)];
        let z = parse_eczn(0x9876, &subs);
        assert_eq!(z.editor_id, "NcrZone");
        assert_eq!(z.owner_form, 0x0001_CAFE);
        assert_eq!(z.rank, 3);
        assert_eq!(z.min_level, 15);
        assert_eq!(z.flags, 0x02);
    }

    #[test]
    fn parse_lgtm_decodes_xcll_prefix() {
        // Use distinct byte patterns so an off-by-one on any field
        // surfaces as a visible assertion failure.
        let mut data = Vec::with_capacity(92);
        data.extend_from_slice(&[80, 82, 85, 0]); // ambient
        data.extend_from_slice(&[200, 195, 180, 0]); // directional
        data.extend_from_slice(&[40, 45, 50, 0]); // fog color
        data.extend_from_slice(&64.0f32.to_le_bytes()); // fog near
        data.extend_from_slice(&4000.0f32.to_le_bytes()); // fog far
        data.extend_from_slice(&15i32.to_le_bytes()); // rot X
        data.extend_from_slice(&(-30i32).to_le_bytes()); // rot Y
        data.extend_from_slice(&0.5f32.to_le_bytes()); // dir fade
        data.extend_from_slice(&6000.0f32.to_le_bytes()); // fog clip
        data.extend_from_slice(&1.25f32.to_le_bytes()); // fog power
        data.extend_from_slice(&[0xAA; 32]); // DATA ambient bundle is unused
        data.extend_from_slice(&[90, 91, 92, 0]); // fog far color
        data.extend_from_slice(&0.75f32.to_le_bytes()); // fog max
        data.extend_from_slice(&512.0f32.to_le_bytes()); // light fade begin
        data.extend_from_slice(&1024.0f32.to_le_bytes()); // light fade end
        data.extend_from_slice(&0u32.to_le_bytes()); // unused

        let mut dalc = Vec::with_capacity(32);
        for face in 0u8..6 {
            let base = 10 + face * 10;
            dalc.extend_from_slice(&[base, base + 1, base + 2, 0]);
        }
        dalc.extend_from_slice(&[210, 211, 212, 128]);
        dalc.extend_from_slice(&3.5f32.to_le_bytes());

        let subs = vec![
            sub(b"EDID", b"LgtmInteriorDim\0"),
            sub(b"DATA", &data),
            sub(b"DALC", &dalc),
        ];
        let l = parse_lgtm(0xDEAD, &subs);
        assert_eq!(l.editor_id, "LgtmInteriorDim");
        assert!((l.ambient[0] - 80.0 / 255.0).abs() < 1e-6);
        assert!((l.directional[1] - 195.0 / 255.0).abs() < 1e-6);
        assert!((l.fog_color[2] - 50.0 / 255.0).abs() < 1e-6);
        assert_eq!(l.fog_near, 64.0);
        assert_eq!(l.fog_far, 4000.0);
        assert!((l.directional_rotation[0] - 15.0f32.to_radians()).abs() < 1e-6);
        assert!((l.directional_rotation[1] - (-30.0f32).to_radians()).abs() < 1e-6);
        assert_eq!(l.directional_fade, Some(0.5));
        assert_eq!(l.fog_clip, Some(6000.0));
        assert_eq!(l.fog_power, Some(1.25));
        assert_eq!(
            l.fog_far_color,
            Some([90.0 / 255.0, 91.0 / 255.0, 92.0 / 255.0])
        );
        assert_eq!(l.fog_max, Some(0.75));
        assert_eq!(l.light_fade_begin, Some(512.0));
        assert_eq!(l.light_fade_end, Some(1024.0));
        let cube = l.directional_ambient.expect("DALC ambient cube");
        assert_eq!(cube[0], [10.0 / 255.0, 11.0 / 255.0, 12.0 / 255.0]);
        assert_eq!(cube[5], [60.0 / 255.0, 61.0 / 255.0, 62.0 / 255.0]);
        assert_eq!(
            l.specular_color,
            Some([210.0 / 255.0, 211.0 / 255.0, 212.0 / 255.0])
        );
        assert_eq!(l.specular_alpha, Some(128.0 / 255.0));
        assert_eq!(l.fresnel_power, Some(3.5));
    }

    /// Regression for #624 / SK-D6-NEW-03. IMGS records were dropped
    /// on the parse_esm catch-all skip pre-fix; this tests the stub
    /// parser captures EDID + raw DNAM so XCIM cross-references can
    /// resolve through `EsmIndex.image_spaces`.
    #[test]
    fn parse_imgs_captures_edid_and_dnam_payload() {
        // 56-byte DNAM patterned with distinct bytes so a future
        // field decoder catches misalignment vs the captured raw
        // payload. Vanilla FO3/FNV ship the 56-byte form; Skyrim
        // extends to 152 — the stub captures whatever DNAM length
        // the file ships with.
        let dnam: Vec<u8> = (0u8..56).collect();
        let subs = vec![sub(b"EDID", b"InteriorWarmDim\0"), sub(b"DNAM", &dnam)];
        let imgs = parse_imgs(0x000A_1234, &subs);
        assert_eq!(imgs.form_id, 0x000A_1234);
        assert_eq!(imgs.editor_id, "InteriorWarmDim");
        assert_eq!(imgs.dnam_raw.as_deref(), Some(dnam.as_slice()));
    }

    /// Companion: an IMGS record with no DNAM (legacy FO3 entries)
    /// captures EDID and leaves `dnam_raw` at None — pinning the
    /// stub's "best-effort capture" semantics so a future consumer
    /// doesn't have to guard against the absent case downstream.
    #[test]
    fn parse_imgs_without_dnam_leaves_payload_none() {
        let subs = vec![sub(b"EDID", b"LegacyImagespace\0")];
        let imgs = parse_imgs(0x000A_5678, &subs);
        assert_eq!(imgs.editor_id, "LegacyImagespace");
        assert!(imgs.dnam_raw.is_none());
    }

    #[test]
    fn parse_lgtm_short_data_returns_defaults() {
        // DATA under 20 bytes → all field captures short-circuit.
        let subs = vec![sub(b"EDID", b"ShortLgtm\0"), sub(b"DATA", &[1, 2, 3, 4])];
        let l = parse_lgtm(0xBEEF, &subs);
        assert_eq!(l.editor_id, "ShortLgtm");
        assert_eq!(l.ambient, [0.0; 3]);
        assert_eq!(l.fog_near, 0.0);
        assert!(l.directional_fade.is_none());
    }
    #[test]
    fn parse_acti_extracts_scri_and_model() {
        let subs = vec![
            sub(b"EDID", b"NukaColaMachine01\0"),
            sub(b"FULL", b"Nuka-Cola Machine\0"),
            sub(b"MODL", b"activators\\nukacolamachine01.nif\0"),
            sub(b"SCRI", &0x0010_ABCDu32.to_le_bytes()),
            sub(b"SNAM", &0x0009_0000u32.to_le_bytes()),
        ];
        let a = parse_acti(0x0002_9E7A, &subs, &None);
        assert_eq!(a.editor_id, "NukaColaMachine01");
        assert_eq!(a.full_name, "Nuka-Cola Machine");
        assert_eq!(a.model_path, "activators\\nukacolamachine01.nif");
        assert_eq!(a.script_form_id, 0x0010_ABCD);
        assert_eq!(a.sound_form_id, 0x0009_0000);
        // Radio form defaults to 0 when RNAM/RADR absent.
        assert_eq!(a.radio_form_id, 0);
    }

    /// TD2-109 / #2068 — `parse_acti` now sources its EDID/FULL/MODL/SCRI/
    /// VMAD bundle from `CommonNamedFields::from_subs`, which means each
    /// field is copied across by hand into `ActiRecord`. Every other field
    /// is asserted by `parse_acti_extracts_scri_and_model` above; the
    /// decoded VMAD attachment was not covered by anything, so a dropped
    /// `script_instance` mapping would have gone unnoticed. Activators are
    /// the most common scripted base record in Skyrim cells, and the M47.2
    /// attach path reads exactly this field.
    #[test]
    fn parse_acti_decodes_vmad_into_script_instance() {
        // version 5, objectFormat 2, 1 script "DoorScript", 0 props —
        // same shape as `common::tests::common_named_fields_decodes_vmad_script_instance`.
        let mut vmad = Vec::new();
        vmad.extend_from_slice(&5i16.to_le_bytes());
        vmad.extend_from_slice(&2i16.to_le_bytes());
        vmad.extend_from_slice(&1u16.to_le_bytes());
        let name = b"DoorScript";
        vmad.extend_from_slice(&(name.len() as u16).to_le_bytes());
        vmad.extend_from_slice(name);
        vmad.push(0); // script status
        vmad.extend_from_slice(&0u16.to_le_bytes()); // propCount = 0

        let subs = vec![
            sub(b"EDID", b"SkyrimLever01\0"),
            sub(b"MODL", b"clutter\\lever01.nif\0"),
            sub(b"VMAD", &vmad),
        ];
        let a = parse_acti(0x0001_0000, &subs, &None);
        assert_eq!(a.editor_id, "SkyrimLever01");
        assert_eq!(a.model_path, "clutter\\lever01.nif");
        let inst = a
            .script_instance
            .expect("VMAD must survive the CommonNamedFields hand-off");
        assert_eq!(inst.scripts.len(), 1);
        assert_eq!(inst.scripts[0].name, "DoorScript");
        // Skyrim activators carry VMAD *instead of* SCRI.
        assert_eq!(a.script_form_id, 0);
    }

    #[test]
    fn parse_term_extracts_password_footer_menu() {
        let subs = vec![
            sub(b"EDID", b"Vault21OverseerTerminal\0"),
            sub(b"FULL", b"Overseer's Terminal\0"),
            sub(b"MODL", b"clutter\\junk\\terminal01.nif\0"),
            sub(b"ANAM", b"tranquility\0"),
            sub(b"DNAM", b"Welcome, Overseer. Vault 21 online.\0"),
            sub(b"BSIZ", &[1u8]),
            sub(b"MNAM", b"Open Vault Door\0"),
            sub(b"MNAM", b"View Resident Log\0"),
            sub(b"MNAM", b"Disable Security\0"),
            sub(b"SCRI", &0x0004_2CD2u32.to_le_bytes()),
        ];
        let t = parse_term(0x0004_2424, &subs, &None);
        assert_eq!(t.editor_id, "Vault21OverseerTerminal");
        assert_eq!(t.password, "tranquility");
        assert!(t.footer_text.starts_with("Welcome, Overseer"));
        assert_eq!(t.body_size, 1);
        assert_eq!(t.menu_items.len(), 3);
        assert_eq!(t.menu_items[0], "Open Vault Door");
        assert_eq!(t.menu_items[2], "Disable Security");
        assert_eq!(t.script_form_id, 0x0004_2CD2);
    }
    #[test]
    fn parse_term_unlocked_terminal_has_empty_password() {
        // Tutorial / ambient terminals often ship without ANAM; stub
        // must tolerate that without panicking.
        let subs = vec![
            sub(b"EDID", b"GoodspringsSchoolTerminal\0"),
            sub(b"FULL", b"School Terminal\0"),
            sub(b"DNAM", b"Primer by Mr. Goodsprings.\0"),
        ];
        let t = parse_term(0x0008_1111, &subs, &None);
        assert!(t.password.is_empty());
        assert_eq!(t.body_size, 0);
        assert!(t.menu_items.is_empty());
    }

    /// Regression for #2663 (SCR-D7-NEW11-02) — `parse_term` used to
    /// discard `CommonNamedFields::script_instance` even though the
    /// shared helper fully decoded it (mirrors
    /// `parse_acti_decodes_vmad_into_script_instance`). FO4 ships 207
    /// VMAD-bearing TERM records — the parser's own "TERM is FO3/FNV-only"
    /// comment justifying the drop was factually wrong.
    #[test]
    fn parse_term_decodes_vmad_into_script_instance() {
        // version 5, objectFormat 2, 1 script "TerminalMenuScript", 0 props.
        let mut vmad = Vec::new();
        vmad.extend_from_slice(&5i16.to_le_bytes());
        vmad.extend_from_slice(&2i16.to_le_bytes());
        vmad.extend_from_slice(&1u16.to_le_bytes());
        let name = b"TerminalMenuScript";
        vmad.extend_from_slice(&(name.len() as u16).to_le_bytes());
        vmad.extend_from_slice(name);
        vmad.push(0); // script status
        vmad.extend_from_slice(&0u16.to_le_bytes()); // propCount = 0

        let subs = vec![
            sub(b"EDID", b"VRWorkshopShared_VRTerminalMusicSubMenu\0"),
            sub(b"MODL", b"setdressing\\workshop\\terminal01.nif\0"),
            sub(b"VMAD", &vmad),
        ];
        let t = parse_term(0x0002_5001, &subs, &None);
        assert_eq!(t.editor_id, "VRWorkshopShared_VRTerminalMusicSubMenu");
        let inst = t
            .script_instance
            .expect("VMAD must survive the CommonNamedFields hand-off (#2663)");
        assert_eq!(inst.scripts.len(), 1);
        assert_eq!(inst.scripts[0].name, "TerminalMenuScript");
    }

    // ── #808 / FNV-D2-NEW-01 stubs ─────────────────────────────────
}

#[cfg(test)]
mod regn_tests {
    use super::*;

    fn sub(sig: &[u8; 4], data: &[u8]) -> SubRecord {
        SubRecord {
            sub_type: *sig,
            data: data.to_vec(),
        }
    }

    /// `RDAT` header as established from shipped data: type u32, flags u8,
    /// priority u8, then a half-word that is zero in all 788 corpus entries.
    fn rdat(kind: u32, flags: u8, priority: u8) -> SubRecord {
        let mut d = kind.to_le_bytes().to_vec();
        d.push(flags);
        d.push(priority);
        d.extend_from_slice(&[0, 0]);
        sub(b"RDAT", &d)
    }

    #[test]
    fn rdat_header_fields_are_decoded() {
        let r = parse_regn(1, &[rdat(3, 1, 75)], &None);
        assert_eq!(r.entries.len(), 1);
        assert_eq!(r.entries[0].kind, RegionDataKind::Weather);
        assert_eq!(r.entries[0].flags, 1);
        assert_eq!(r.entries[0].priority, 75);
    }

    #[test]
    fn every_observed_type_word_maps_to_a_kind() {
        // The seven type words the corpus actually authors.
        let expected = [
            (2, RegionDataKind::Objects),
            (3, RegionDataKind::Weather),
            (4, RegionDataKind::Map),
            (5, RegionDataKind::Landscape),
            (6, RegionDataKind::Grass),
            (7, RegionDataKind::Sound),
            (8, RegionDataKind::Imposter),
        ];
        for (word, kind) in expected {
            assert_eq!(RegionDataKind::from_word(word), kind, "type {word}");
        }
        // An unmodelled type must surface its word rather than vanish.
        assert_eq!(RegionDataKind::from_word(99), RegionDataKind::Unknown(99));
    }

    #[test]
    fn payload_sub_records_attach_to_the_open_section() {
        // The chain is positional: sub-records belong to the most recent
        // RDAT. Getting this wrong silently reassigns every payload.
        let mut weather = 0x0017_3564u32.to_le_bytes().to_vec();
        weather.extend_from_slice(&85u32.to_le_bytes());
        weather.extend_from_slice(&0u32.to_le_bytes());
        let record = parse_regn(
            1,
            &[
                rdat(3, 0, 50),
                sub(b"RDWT", &weather),
                rdat(4, 0, 50),
                sub(b"RDMP", b"Mojave Wasteland\0"),
            ],
            &None,
        );
        assert_eq!(record.entries.len(), 2);
        assert!(matches!(
            record.entries[0].payload,
            RegionDataPayload::Weather(ref w) if w.len() == 1 && w[0].chance == 85
        ));
        assert!(matches!(
            record.entries[1].payload,
            RegionDataPayload::Map(ref m) if m == "Mojave Wasteland"
        ));
    }

    #[test]
    fn weather_row_stride_follows_the_payload_not_a_game_flag() {
        // Oblivion authors 8-byte rows, FO3/FNV/Skyrim 12-byte. Deciding from
        // the payload length keeps the parser free of a game branch, per the
        // format-abstraction doctrine.
        let mut short = 0xAAu32.to_le_bytes().to_vec();
        short.extend_from_slice(&40u32.to_le_bytes());
        let r = parse_regn(1, &[rdat(3, 0, 50), sub(b"RDWT", &short)], &None);
        match &r.entries[0].payload {
            RegionDataPayload::Weather(w) => {
                assert_eq!(w.len(), 1);
                assert_eq!(w[0].chance, 40);
                assert_eq!(w[0].global_form, None, "Oblivion rows carry no GLOB");
            }
            other => panic!("{other:?}"),
        }

        let mut long = 0xBBu32.to_le_bytes().to_vec();
        long.extend_from_slice(&100u32.to_le_bytes());
        long.extend_from_slice(&0x1234u32.to_le_bytes());
        let r = parse_regn(1, &[rdat(3, 0, 50), sub(b"RDWT", &long)], &None);
        match &r.entries[0].payload {
            RegionDataPayload::Weather(w) => {
                assert_eq!(w.len(), 1);
                assert_eq!(w[0].global_form, Some(0x1234));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn sound_sections_accumulate_across_their_sub_records() {
        // A sound section is several sub-records that must merge, not
        // overwrite: FNV authors RDSB + RDSD + RDSI in one section.
        let mut sounds = 0x0014_1866u32.to_le_bytes().to_vec();
        sounds.extend_from_slice(&15u32.to_le_bytes());
        sounds.extend_from_slice(&200_000u32.to_le_bytes());
        let r = parse_regn(
            1,
            &[
                rdat(7, 0, 50),
                sub(b"RDSB", &0x99u32.to_le_bytes()),
                sub(b"RDSD", &sounds),
                sub(b"RDSI", &0x77u32.to_le_bytes()),
            ],
            &None,
        );
        match &r.entries[0].payload {
            RegionDataPayload::Sound {
                music,
                incidental,
                sounds,
            } => {
                assert_eq!(*music, Some(0x99));
                assert_eq!(*incidental, Some(0x77));
                assert_eq!(sounds.len(), 1);
                assert_eq!(sounds[0].sound_form, 0x0014_1866);
                assert_eq!(sounds[0].flags, 15);
                assert_eq!(sounds[0].chance_raw, 200_000);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn skyrim_music_signature_merges_the_same_way() {
        let r = parse_regn(
            1,
            &[rdat(7, 0, 50), sub(b"RDMO", &0x4242u32.to_le_bytes())],
            &None,
        );
        assert!(matches!(
            r.entries[0].payload,
            RegionDataPayload::Sound {
                music: Some(0x4242),
                ..
            }
        ));
    }

    #[test]
    fn areas_pair_edge_falloff_with_their_point_list() {
        let mut points = Vec::new();
        for (x, y) in [(0.0f32, 0.0f32), (4096.0, 0.0), (4096.0, 4096.0)] {
            points.extend_from_slice(&x.to_le_bytes());
            points.extend_from_slice(&y.to_le_bytes());
        }
        let r = parse_regn(
            1,
            &[
                sub(b"RPLI", &128u32.to_le_bytes()),
                sub(b"RPLD", &points),
                sub(b"RPLI", &64u32.to_le_bytes()),
                sub(b"RPLD", &points[..16]),
            ],
            &None,
        );
        assert_eq!(r.areas.len(), 2);
        assert_eq!(r.areas[0].edge_fall_off, 128);
        assert_eq!(r.areas[0].points.len(), 3);
        assert_eq!(r.areas[0].points[1], (4096.0, 0.0));
        assert_eq!(r.areas[1].edge_fall_off, 64);
        assert_eq!(r.areas[1].points.len(), 2);
    }

    #[test]
    fn a_point_list_without_edge_falloff_still_keeps_its_geometry() {
        // Dropping the polygon because RPLI was absent would silently shrink
        // the region, which is worse than a zero falloff.
        let mut points = 1.0f32.to_le_bytes().to_vec();
        points.extend_from_slice(&2.0f32.to_le_bytes());
        let r = parse_regn(1, &[sub(b"RPLD", &points)], &None);
        assert_eq!(r.areas.len(), 1);
        assert_eq!(r.areas[0].points, vec![(1.0, 2.0)]);
        assert_eq!(r.areas[0].edge_fall_off, 0);
    }

    #[test]
    fn entries_sort_by_authored_priority() {
        // EX-16 requires deterministic priority, and it is authored in RDAT
        // rather than derived from record order.
        let r = parse_regn(
            1,
            &[
                rdat(7, 0, 50),
                rdat(7, 0, 100),
                rdat(7, 0, 75),
                rdat(3, 0, 99),
            ],
            &None,
        );
        let sounds = r.entries_by_priority(RegionDataKind::Sound);
        assert_eq!(
            sounds.iter().map(|e| e.priority).collect::<Vec<_>>(),
            vec![100, 75, 50]
        );
        // Filtering is by kind — the higher-priority weather entry must not
        // leak into the sound query.
        assert_eq!(sounds.len(), 3);
    }

    #[test]
    fn equal_priorities_keep_authored_order() {
        // Nothing in the record distinguishes two entries at equal priority,
        // so a stable sort is the only defensible tie-break.
        let r = parse_regn(1, &[rdat(2, 0, 50), rdat(2, 1, 50), rdat(2, 0, 50)], &None);
        let objects = r.entries_by_priority(RegionDataKind::Objects);
        assert_eq!(
            objects.iter().map(|e| e.flags).collect::<Vec<_>>(),
            vec![0, 1, 0]
        );
    }

    #[test]
    fn truncated_payloads_drop_the_short_row_not_the_region() {
        // One malformed plugin must not take the whole region with it.
        let mut ragged = 0xAAu32.to_le_bytes().to_vec();
        ragged.extend_from_slice(&40u32.to_le_bytes());
        ragged.extend_from_slice(&[0xFF]); // 9 bytes: one 8-byte row + junk
        let r = parse_regn(1, &[rdat(3, 0, 50), sub(b"RDWT", &ragged)], &None);
        match &r.entries[0].payload {
            RegionDataPayload::Weather(w) => assert_eq!(w.len(), 1),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_rdat_with_no_payload_stays_empty_rather_than_absent() {
        // Skyrim authors 69 empty RDOT sections; they must still appear so a
        // consumer sees the declared-but-empty distinction.
        let r = parse_regn(1, &[rdat(2, 0, 50), sub(b"RDOT", &[])], &None);
        assert_eq!(r.entries.len(), 1);
        assert_eq!(r.entries[0].kind, RegionDataKind::Objects);
        assert!(matches!(
            r.entries[0].payload,
            RegionDataPayload::Objects(ref v) if v.is_empty()
        ));
    }

    #[test]
    fn payloads_before_any_rdat_are_ignored_not_misattributed() {
        let r = parse_regn(1, &[sub(b"RDMP", b"orphan\0"), rdat(4, 0, 50)], &None);
        assert_eq!(r.entries.len(), 1);
        assert_eq!(r.entries[0].payload, RegionDataPayload::Empty);
    }

    #[test]
    fn ambiguous_weather_lengths_resolve_by_validating_the_chance() {
        // 24 bytes is divisible by both strides. A divisibility tie-break got
        // this wrong and produced 65 Oblivion rows with chance > 100 in a
        // corpus sweep; `chance` is a percentage, so it is its own checksum.
        let mut oblivion = Vec::new();
        for (form, chance) in [(0xAAu32, 40u32), (0xBB, 50), (0xCC, 10)] {
            oblivion.extend_from_slice(&form.to_le_bytes());
            oblivion.extend_from_slice(&chance.to_le_bytes());
        }
        assert_eq!(oblivion.len(), 24);
        let r = parse_regn(1, &[rdat(3, 0, 50), sub(b"RDWT", &oblivion)], &None);
        match &r.entries[0].payload {
            RegionDataPayload::Weather(w) => {
                assert_eq!(w.len(), 3, "24B of 8-byte rows must split into 3");
                assert!(w.iter().all(|x| x.chance <= 100));
                assert!(w.iter().all(|x| x.global_form.is_none()));
            }
            other => panic!("{other:?}"),
        }

        // The same length as two 12-byte rows: reading it as three 8-byte rows
        // puts a FormID in the second row's chance, which must fail validation
        // and fall through to the long form.
        let mut long = Vec::new();
        for (form, chance, global) in [(0xAAu32, 40u32, 0x9999u32), (0xBB, 50, 0x8888)] {
            long.extend_from_slice(&form.to_le_bytes());
            long.extend_from_slice(&chance.to_le_bytes());
            long.extend_from_slice(&global.to_le_bytes());
        }
        assert_eq!(long.len(), 24);
        let r = parse_regn(1, &[rdat(3, 0, 50), sub(b"RDWT", &long)], &None);
        match &r.entries[0].payload {
            RegionDataPayload::Weather(w) => {
                assert_eq!(w.len(), 2);
                assert_eq!(w[0].global_form, Some(0x9999));
                assert_eq!(w[1].chance, 50);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn skyrim_localised_map_names_are_string_ids_not_text() {
        // Skyrim's RDMP is a 4-byte .STRINGS id; reading it as inline text
        // yields control characters, which a corpus sweep surfaced as garbage
        // map names before this branch existed.
        let r = parse_regn(
            1,
            &[rdat(4, 0, 50), sub(b"RDMP", &0x0001_1086u32.to_le_bytes())],
            &None,
        );
        assert_eq!(
            r.entries[0].payload,
            RegionDataPayload::MapStringId(0x0001_1086)
        );

        // …while the inline form still parses as text.
        let r = parse_regn(1, &[rdat(4, 0, 50), sub(b"RDMP", b"Arefu\0")], &None);
        assert_eq!(
            r.entries[0].payload,
            RegionDataPayload::Map("Arefu".to_string())
        );
    }

    #[test]
    fn existing_fields_still_parse() {
        let r = parse_regn(
            7,
            &[
                sub(b"EDID", b"MojaveRegion\0"),
                sub(b"WNAM", &0x1234u32.to_le_bytes()),
                sub(b"RCLR", &[10, 20, 30, 255]),
            ],
            &None,
        );
        assert_eq!(r.form_id, 7);
        assert_eq!(r.editor_id, "MojaveRegion");
        assert_eq!(r.weather_form, Some(0x1234));
        assert_eq!(r.color, Some([10, 20, 30]));
    }
}

#[cfg(test)]
mod navm_tests {
    use super::*;

    fn sub(sig: &[u8; 4], data: &[u8]) -> SubRecord {
        SubRecord {
            sub_type: *sig,
            data: data.to_vec(),
        }
    }

    fn verts(n: usize) -> SubRecord {
        let mut d = Vec::new();
        for i in 0..n {
            for axis in 0..3 {
                d.extend_from_slice(&((i * 3 + axis) as f32).to_le_bytes());
            }
        }
        sub(b"NVVX", &d)
    }

    fn tri(v: [u16; 3], links: [u16; 3], flags: u32) -> Vec<u8> {
        let mut d = Vec::new();
        for x in v.iter().chain(links.iter()) {
            d.extend_from_slice(&x.to_le_bytes());
        }
        d.extend_from_slice(&flags.to_le_bytes());
        d
    }

    #[test]
    fn vertices_decode_as_f32_triples() {
        let r = parse_navm(1, &[verts(3)], &None);
        assert_eq!(r.vertices.len(), 3);
        assert_eq!(r.vertices[0], [0.0, 1.0, 2.0]);
        assert_eq!(r.vertices[2], [6.0, 7.0, 8.0]);
    }

    #[test]
    fn triangles_decode_indices_links_and_flags() {
        let data = tri([236, 683, 534], [506, u16::MAX, 1], 0x0000_0C00);
        let r = parse_navm(1, &[sub(b"NVTR", &data)], &None);
        assert_eq!(r.triangles.len(), 1);
        let t = r.triangles[0];
        assert_eq!(t.vertices, [236, 683, 534]);
        assert_eq!(t.flags, 0x0000_0C00);
        // 0xFFFF is the authored border sentinel — modelling it as an index
        // would send a path walker into element 65535.
        assert_eq!(t.edge_neighbours, [Some(506), None, Some(1)]);
    }

    /// #3300 — `NVDP` is stride **8** (not the packed form's 10), and `DATA`
    /// word 5 is its row count. Bytes are a real FO3 row.
    #[test]
    fn door_triangles_decode_from_the_typed_form() {
        // form=0x00003A73, triangle=26, trailing u16=0.
        let row = [0x73u8, 0x3a, 0x00, 0x00, 0x1a, 0x00, 0x00, 0x00];
        let r = parse_navm(1, &[sub(b"NVDP", &row)], &None);
        assert_eq!(r.door_triangles.len(), 1);
        let d = &r.door_triangles[0];
        assert_eq!(d.door_form_id, 0x0000_3A73);
        assert_eq!(d.triangle, 26);
        assert_eq!(d.unknown, 0);
    }

    /// `NVCA` is a bare `u16` triangle-index list (stride 2); `DATA` word 4 is
    /// its count. Verified in range on 60,534/60,534 FO3 and 15,791/15,791 FNV
    /// entries.
    #[test]
    fn cover_triangles_decode_as_bare_indices() {
        let data = [7u8, 0, 0, 1, 255, 255];
        let r = parse_navm(1, &[sub(b"NVCA", &data)], &None);
        assert_eq!(r.cover_triangles, vec![7, 256, 65535]);
    }

    /// `NVGD`: `u32 divisor`, eight `f32`, then `divisor^2` **`u16`-counted**
    /// index lists. The `u16` count is the one divergence from the packed
    /// form's tail block, which counts with a `u32` — decoding it as `u32`
    /// reconciles 0 of 11,969 shipped meshes, which is how the difference was
    /// found.
    #[test]
    fn grid_accel_decodes_a_divisor_two_lattice() {
        let mut d = Vec::new();
        d.extend_from_slice(&2u32.to_le_bytes());
        for f in [32.0f32, 42.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0] {
            d.extend_from_slice(&f.to_le_bytes());
        }
        // Four cells: [0,1], [], [2], [3,4,5].
        for cell in [vec![0u16, 1], vec![], vec![2], vec![3, 4, 5]] {
            d.extend_from_slice(&(cell.len() as u16).to_le_bytes());
            for t in cell {
                d.extend_from_slice(&t.to_le_bytes());
            }
        }
        let r = parse_navm(1, &[sub(b"NVGD", &d)], &None);
        let g = r.grid_accel.expect("lattice must decode");
        assert_eq!(g.divisor, 2);
        assert_eq!(g.bounds, [32.0, 42.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(g.cells.len(), 4, "divisor^2 cells");
        assert_eq!(g.cells[0], vec![0, 1]);
        assert!(g.cells[1].is_empty());
        assert_eq!(g.cells[2], vec![2]);
        assert_eq!(g.cells[3], vec![3, 4, 5]);
    }

    /// All-or-nothing: a payload that does not consume exactly is rejected
    /// rather than half-filled, matching `decode_nvnm`'s posture.
    #[test]
    fn grid_accel_rejects_a_payload_that_does_not_reconcile() {
        let mut d = Vec::new();
        d.extend_from_slice(&1u32.to_le_bytes());
        for _ in 0..8 {
            d.extend_from_slice(&0f32.to_le_bytes());
        }
        d.extend_from_slice(&1u16.to_le_bytes()); // count = 1
        d.extend_from_slice(&5u16.to_le_bytes()); // the index
        d.push(0xAA); // one trailing byte too many
        assert!(parse_navm(1, &[sub(b"NVGD", &d)], &None)
            .grid_accel
            .is_none());

        // A divisor past the bound must not drive a divisor^2 walk.
        let mut big = Vec::new();
        big.extend_from_slice(&(NVNM_MAX_DIVISOR + 1).to_le_bytes());
        big.extend_from_slice(&[0u8; 32]);
        assert!(parse_navm(1, &[sub(b"NVGD", &big)], &None)
            .grid_accel
            .is_none());
    }

    #[test]
    fn external_connections_expose_the_neighbouring_mesh() {
        // The bytes of a real FNV NVEX row.
        let row = [2u8, 0, 0, 0, 252, 70, 22, 0, 252, 0];
        let r = parse_navm(1, &[sub(b"NVEX", &row)], &None);
        assert_eq!(r.external_connections.len(), 1);
        let c = r.external_connections[0];
        assert_eq!(c.unknown, 2);
        assert_eq!(c.mesh_form, 0x0016_46FC);
        assert_eq!(c.triangle, 252);
    }

    #[test]
    fn linked_meshes_dedups_and_sorts() {
        // Two triangles of the same mesh link to one neighbour; a consumer
        // asking "what must be resident" wants the mesh once.
        let mut d = Vec::new();
        for (u, form, tri) in [(2u32, 0xAAu32, 1u16), (1, 0xAA, 9), (1, 0x22, 3)] {
            d.extend_from_slice(&u.to_le_bytes());
            d.extend_from_slice(&form.to_le_bytes());
            d.extend_from_slice(&tri.to_le_bytes());
        }
        let r = parse_navm(1, &[sub(b"NVEX", &d)], &None);
        assert_eq!(r.linked_meshes(), vec![0x22, 0xAA]);
    }

    #[test]
    fn data_header_supplies_the_parent_cell() {
        let mut d = 0x0012_06D8u32.to_le_bytes().to_vec();
        d.extend_from_slice(&687u32.to_le_bytes());
        d.extend_from_slice(&746u32.to_le_bytes());
        d.extend_from_slice(&[0u8; 12]);
        assert_eq!(d.len(), 24);
        let r = parse_navm(1, &[sub(b"DATA", &d)], &None);
        assert_eq!(r.cell_form, Some(0x0012_06D8));
    }

    #[test]
    fn index_range_check_catches_a_dangling_vertex() {
        // Holds for all 11,969 FO3+FNV meshes, so a violation means a corrupt
        // plugin or a decode regression — worth being able to assert.
        let ok = parse_navm(
            1,
            &[verts(3), sub(b"NVTR", &tri([0, 1, 2], [0, 0, 0], 0))],
            &None,
        );
        assert!(ok.indices_are_in_range());
        let bad = parse_navm(
            1,
            &[verts(3), sub(b"NVTR", &tri([0, 1, 99], [0, 0, 0], 0))],
            &None,
        );
        assert!(!bad.indices_are_in_range());
    }

    /// Build a Creation-Engine `NVNM` body (#2738). `location` is either the
    /// interior cell FormID or the exterior `(x, y)` grid; `divisor` sizes
    /// the trailing segment grid.
    fn nvnm(
        worldspace: u32,
        location: Result<u32, (i16, i16)>,
        vertices: &[[f32; 3]],
        triangles: &[[u8; 16]],
        links: &[[u8; 10]],
        divisor: u32,
    ) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&7u32.to_le_bytes()); // unknown 0
        d.extend_from_slice(&0u32.to_le_bytes()); // unknown 1
        d.extend_from_slice(&worldspace.to_le_bytes());
        match location {
            Ok(cell) => d.extend_from_slice(&cell.to_le_bytes()),
            Err((x, y)) => {
                // y before x — the order the corpus establishes.
                d.extend_from_slice(&y.to_le_bytes());
                d.extend_from_slice(&x.to_le_bytes());
            }
        }
        d.extend_from_slice(&(vertices.len() as u32).to_le_bytes());
        for v in vertices {
            for c in v {
                d.extend_from_slice(&c.to_le_bytes());
            }
        }
        d.extend_from_slice(&(triangles.len() as u32).to_le_bytes());
        for t in triangles {
            d.extend_from_slice(t);
        }
        d.extend_from_slice(&(links.len() as u32).to_le_bytes());
        for l in links {
            d.extend_from_slice(l);
        }
        d.extend_from_slice(&0u32.to_le_bytes()); // door triangles
        d.extend_from_slice(&0u32.to_le_bytes()); // cover triangles
        d.extend_from_slice(&divisor.to_le_bytes());
        d.extend_from_slice(&[0u8; 32]); // max X/Y dist + min/max XYZ
        for _ in 0..(divisor * divisor) {
            d.extend_from_slice(&0u32.to_le_bytes()); // empty segment list
        }
        d
    }

    /// #2738 — the packed Creation-Engine form decodes into the same
    /// canonical fields the Gamebryo typed form fills, so a consumer never
    /// branches per game. Verified against 19,551 shipped Skyrim-era meshes.
    #[test]
    fn creation_engine_packed_mesh_decodes_into_the_canonical_fields() {
        let blob = nvnm(
            0x0000_003C,
            Err((7, -3)),
            &[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]],
            &[tri([0, 1, 2], [1, u16::MAX, 2], 0xABCD_1234)
                .try_into()
                .unwrap()],
            &[{
                let mut row = [0u8; 10];
                row[0..4].copy_from_slice(&1u32.to_le_bytes());
                row[4..8].copy_from_slice(&0x0002_EE41u32.to_le_bytes());
                row[8..10].copy_from_slice(&5u16.to_le_bytes());
                row
            }],
            2,
        );
        let r = parse_navm(1, &[sub(b"NVNM", &blob)], &None);

        assert_eq!(r.worldspace_form, Some(0x0000_003C));
        assert_eq!(
            r.grid,
            Some((7, -3)),
            "exterior tiles carry a grid, y first"
        );
        assert_eq!(r.cell_form, None, "…and no cell FormID");
        assert_eq!(r.vertices.len(), 3);
        assert_eq!(r.vertices[2], [7.0, 8.0, 9.0]);
        assert_eq!(r.triangles.len(), 1);
        assert_eq!(r.triangles[0].vertices, [0, 1, 2]);
        assert_eq!(
            r.triangles[0].edge_neighbours,
            [Some(1), None, Some(2)],
            "0xFFFF is the border sentinel here exactly as in NVTR"
        );
        assert_eq!(r.linked_meshes(), vec![0x0002_EE41]);
        assert!(r.has_decoded_geometry());
        assert!(r.indices_are_in_range());
        assert!(
            r.packed_geometry.is_none(),
            "a body that reconciles must not also keep the blob"
        );
    }

    /// The interior branch: worldspace `0` means word 3 is the owning CELL,
    /// not a grid. Established against 1,526 `Skyrim.esm` interior tiles,
    /// every one of which matched its enclosing cell-children group label.
    #[test]
    fn packed_interior_mesh_carries_its_cell_instead_of_a_grid() {
        let blob = nvnm(0, Ok(0x0001_A2B3), &[[0.0, 0.0, 0.0]], &[], &[], 1);
        let r = parse_navm(1, &[sub(b"NVNM", &blob)], &None);
        assert_eq!(r.worldspace_form, Some(0));
        assert_eq!(r.cell_form, Some(0x0001_A2B3));
        assert_eq!(r.grid, None);
    }

    /// #2738 — Fallout 4 diverges after the shared header (0 of 7,894
    /// `Fallout4.esm` bodies reconcile). The decode is gated on exact
    /// reconciliation rather than a version check, so an unrecognised body
    /// keeps its bytes while the header still locates the tile.
    #[test]
    fn a_body_that_does_not_reconcile_keeps_its_blob_but_still_locates() {
        let mut blob = nvnm(0x0000_003C, Err((4, 9)), &[[0.0, 0.0, 0.0]], &[], &[], 1);
        // Truncate the trailing segment list: header intact, body short.
        blob.truncate(blob.len() - 2);
        let r = parse_navm(1, &[sub(b"NVNM", &blob)], &None);

        assert_eq!(r.worldspace_form, Some(0x0000_003C));
        assert_eq!(r.grid, Some((4, 9)), "the header still locates the tile");
        assert!(!r.has_decoded_geometry(), "…but the body is not guessed at");
        assert_eq!(r.packed_geometry.as_deref(), Some(&blob[..]));
    }

    /// Trailing bytes are as disqualifying as missing ones: a body that
    /// under-consumes means the layout is not the one this decoder knows.
    #[test]
    fn a_body_with_trailing_bytes_is_not_accepted() {
        let mut blob = nvnm(0, Ok(0x10), &[[0.0, 0.0, 0.0]], &[], &[], 1);
        blob.push(0xFF);
        let r = parse_navm(1, &[sub(b"NVNM", &blob)], &None);
        assert!(!r.has_decoded_geometry());
        assert!(r.packed_geometry.is_some());
    }

    /// A garbage `divisor` must not turn into a multi-billion-iteration walk.
    #[test]
    fn an_absurd_segment_divisor_is_rejected_rather_than_walked() {
        let mut blob = nvnm(0, Ok(0x10), &[[0.0, 0.0, 0.0]], &[], &[], 1);
        // Overwrite the divisor word (last 4 + 32 + 4 bytes back) with 0xFFFF.
        let divisor_at = blob.len() - 4 - 32 - 4;
        blob[divisor_at..divisor_at + 4].copy_from_slice(&0xFFFFu32.to_le_bytes());
        let r = parse_navm(1, &[sub(b"NVNM", &blob)], &None);
        assert!(r.packed_geometry.is_some(), "bounded, and no geometry");
        assert!(!r.has_decoded_geometry());
    }

    #[test]
    fn a_truncated_header_retains_the_blob_and_reports_nothing() {
        let r = parse_navm(1, &[sub(b"NVNM", &[1, 2, 3, 4])], &None);
        assert_eq!(r.packed_geometry.as_deref(), Some(&[1u8, 2, 3, 4][..]));
        assert!(!r.has_decoded_geometry());
        assert!(r.vertices.is_empty());
        assert_eq!(r.worldspace_form, None);
    }

    #[test]
    fn a_gamebryo_mesh_reports_decoded_geometry() {
        let r = parse_navm(
            1,
            &[verts(3), sub(b"NVTR", &tri([0, 1, 2], [0, 0, 0], 0))],
            &None,
        );
        assert!(r.has_decoded_geometry());
        assert!(r.packed_geometry.is_none());
    }

    #[test]
    fn oblivion_style_absence_is_not_an_error() {
        // Oblivion authors zero NAVM records; the model must tolerate a mesh
        // that carries nothing rather than assuming geometry exists.
        let r = parse_navm(1, &[sub(b"NVER", &12u32.to_le_bytes())], &None);
        assert_eq!(r.version, 12);
        assert!(!r.has_decoded_geometry());
        assert!(r.linked_meshes().is_empty());
        assert!(r.indices_are_in_range(), "vacuously true with no triangles");
    }

    #[test]
    fn ragged_payloads_are_not_recognised_rather_than_truncated() {
        // #3404 — a non-zero stride remainder used to be silently dropped
        // by `rows()`, keeping the one row that did fit. `rows_exact` now
        // refuses the whole sub-record instead, matching
        // `decode_nvgd`/`decode_nvnm`'s posture: a format revision should
        // surface as "not recognised", not a half-filled lattice.
        let mut d = tri([0, 1, 2], [0, 0, 0], 0);
        d.push(0xFF); // 17 bytes — not a multiple of NVTR's 16-byte stride
        let r = parse_navm(1, &[verts(3), sub(b"NVTR", &d)], &None);
        assert_eq!(r.triangles.len(), 0);
    }

    /// Builds a 24-byte `DATA` payload with the given word-1..5 counts
    /// (vertex, triangle, external, cover, door), word 0 = 0.
    fn data_with_counts(words: [u32; 5]) -> Vec<u8> {
        let mut d = 0u32.to_le_bytes().to_vec();
        for w in words {
            d.extend_from_slice(&w.to_le_bytes());
        }
        d
    }

    #[test]
    fn a_data_header_count_that_matches_is_accepted() {
        // #3404 — the cross-check must not reject the common, reconciling
        // case; only a real mismatch should discard the row list.
        let data = data_with_counts([0, 1, 0, 0, 0]); // triangle count = 1
        let r = parse_navm(
            1,
            &[
                verts(3),
                sub(b"DATA", &data),
                sub(b"NVTR", &tri([0, 1, 2], [0, 0, 0], 0)),
            ],
            &None,
        );
        assert_eq!(r.triangles.len(), 1);
    }

    #[test]
    fn a_data_header_count_mismatch_is_not_recognised_even_with_an_exact_stride() {
        // #3404 — two valid 16-byte NVTR rows (an exact stride multiple)
        // but a `DATA` header claiming only one triangle. The cross-check
        // must win: an exact-multiple payload that still disagrees with
        // the header is exactly the "future format revision" case #3404
        // wants to surface, not silently accept.
        let data = data_with_counts([0, 1, 0, 0, 0]); // triangle count = 1
        let mut two_rows = tri([0, 1, 2], [0, 0, 0], 0);
        two_rows.extend(tri([1, 2, 0], [0, 0, 0], 0));
        let r = parse_navm(
            1,
            &[verts(3), sub(b"DATA", &data), sub(b"NVTR", &two_rows)],
            &None,
        );
        assert_eq!(r.triangles.len(), 0);
    }

    // ── `select_active_region_sound` (EX-16 item 1, #2372) ────────────

    fn sound_entry(priority: u8, music: Option<u32>) -> RegionDataEntry {
        RegionDataEntry {
            kind: RegionDataKind::Sound,
            flags: 0,
            priority,
            payload: RegionDataPayload::Sound {
                music,
                incidental: None,
                sounds: Vec::new(),
            },
        }
    }

    fn weather_entry(priority: u8) -> RegionDataEntry {
        RegionDataEntry {
            kind: RegionDataKind::Weather,
            flags: 0,
            priority,
            payload: RegionDataPayload::Weather(Vec::new()),
        }
    }

    fn regn(form_id: u32, entries: Vec<RegionDataEntry>) -> RegnRecord {
        RegnRecord {
            form_id,
            entries,
            ..Default::default()
        }
    }

    #[test]
    fn picks_the_only_sound_entry_in_a_single_tagging_region() {
        let mut regions = HashMap::new();
        regions.insert(0x10, regn(0x10, vec![sound_entry(50, Some(0xAAAA))]));
        let winner = select_active_region_sound(&[0x10], &regions).expect("one Sound entry");
        assert_eq!(
            winner.payload,
            RegionDataPayload::Sound {
                music: Some(0xAAAA),
                incidental: None,
                sounds: Vec::new(),
            }
        );
    }

    #[test]
    fn higher_priority_wins_across_two_tagging_regions() {
        // A cell tagged by two overlapping REGN polygons — the RDAT
        // priority byte, not region list order, decides the winner.
        let mut regions = HashMap::new();
        regions.insert(0x10, regn(0x10, vec![sound_entry(50, Some(0xAAAA))]));
        regions.insert(0x20, regn(0x20, vec![sound_entry(90, Some(0xBBBB))]));
        // List the lower-priority region FIRST to prove priority, not
        // list order, is what wins.
        let winner =
            select_active_region_sound(&[0x10, 0x20], &regions).expect("higher-priority region");
        assert_eq!(
            winner.payload,
            RegionDataPayload::Sound {
                music: Some(0xBBBB),
                incidental: None,
                sounds: Vec::new(),
            }
        );
    }

    #[test]
    fn ties_keep_region_list_order_then_within_region_order() {
        let mut regions = HashMap::new();
        regions.insert(0x10, regn(0x10, vec![sound_entry(50, Some(0x1111))]));
        regions.insert(0x20, regn(0x20, vec![sound_entry(50, Some(0x2222))]));
        let winner = select_active_region_sound(&[0x10, 0x20], &regions).unwrap();
        assert_eq!(
            winner.payload,
            RegionDataPayload::Sound {
                music: Some(0x1111),
                incidental: None,
                sounds: Vec::new(),
            },
            "equal priority must keep the first-listed region's entry, mirroring \
             entries_by_priority's stable-sort tie-break"
        );
    }

    #[test]
    fn non_sound_entries_never_win_even_at_higher_priority() {
        let mut regions = HashMap::new();
        regions.insert(
            0x10,
            regn(0x10, vec![weather_entry(100), sound_entry(10, Some(0x1))]),
        );
        let winner = select_active_region_sound(&[0x10], &regions).unwrap();
        assert_eq!(winner.kind, RegionDataKind::Sound);
    }

    #[test]
    fn no_tagging_regions_yields_none() {
        let regions = HashMap::new();
        assert!(select_active_region_sound(&[], &regions).is_none());
    }

    #[test]
    fn a_tagging_region_missing_from_the_map_is_skipped_not_a_panic() {
        // The cell's XCLR points at a REGN this parser never saw (bad
        // load order, or a REGN type this parser doesn't model) — must
        // not panic, and other tagging regions still resolve.
        let mut regions = HashMap::new();
        regions.insert(0x20, regn(0x20, vec![sound_entry(50, Some(0x2222))]));
        let winner = select_active_region_sound(&[0x10_u32, 0x20], &regions).unwrap();
        assert_eq!(
            winner.payload,
            RegionDataPayload::Sound {
                music: Some(0x2222),
                incidental: None,
                sounds: Vec::new(),
            }
        );
    }

    #[test]
    fn a_tagging_region_with_no_sound_entries_yields_none() {
        let mut regions = HashMap::new();
        regions.insert(0x10, regn(0x10, vec![weather_entry(100)]));
        assert!(select_active_region_sound(&[0x10], &regions).is_none());
    }
}
