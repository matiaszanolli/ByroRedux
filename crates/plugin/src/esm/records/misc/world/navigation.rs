//! `NAVI` navigation master and `NAVM` navigation meshes.


use byroredux_core::bytes::le_u32_at;
use super::super::super::common::{remap_fid, CommonNamedFields};
use crate::esm::reader::{FormIdRemap, SubRecord};
use crate::esm::sub_reader::SubReader;
use super::{rows, rows_exact};

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
    /// FO3 / FNV `NVCI` *Door Links*: every door `REFR` some navmesh
    /// connection names, flattened over the connections (XCT-ROUTE-003, the
    /// load doors an actor can path through). xEdit `wbDefinitionsFO3.pas`
    /// `NAVI`: `NVCI` is a navmesh FormID, then three `u32`-counted FormID
    /// arrays — *Standard*, *Preferred*, *Door Links*. A row whose arrays do not
    /// consume it exactly is skipped, not guessed at. Empty on games whose
    /// `NAVI` has no `NVCI` (Skyrim+ keep door links in `NAVM`).
    pub door_links: Vec<u32>,
}

/// One `NVCI` row's door links, or `None` when the row does not reconcile
/// (see [`NaviRecord::door_links`]).
fn nvci_door_links(data: &[u8], remap: &Option<FormIdRemap>) -> Option<Vec<u32>> {
    let mut at = 4usize; // the connection's own navmesh
    let mut doors = Vec::new();
    for list in 0..3 {
        let count = le_u32_at(data, at)? as usize;
        at += 4;
        let end = at.checked_add(count.checked_mul(4)?)?;
        if end > data.len() {
            return None;
        }
        if list == 2 {
            doors.extend((0..count).filter_map(|i| le_u32_at(data, at + i * 4)).map(|raw| remap_fid(raw, remap)));
        }
        at = end;
    }
    (at == data.len()).then_some(doors)
}

pub fn parse_navi(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> NaviRecord {
    let mut out = NaviRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    out.editor_id = common.editor_id;
    for sub in subs {
        match &sub.sub_type {
            b"NVER" if sub.data.len() >= 4 => {
                out.version = SubReader::new(&sub.data).u32_or_default();
            }
            b"NVCI" => {
                if let Some(doors) = nvci_door_links(&sub.data, remap) {
                    out.door_links.extend(doors.into_iter().filter(|door| *door != 0));
                }
            }
            _ => {}
        }
    }
    out.door_links.sort_unstable();
    out.door_links.dedup();
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
        .map(|s| std::array::from_fn(|i| le_u32_at(&s.data, i * 4).unwrap_or(0)));
    for sub in subs {
        let data = &sub.data;
        match &sub.sub_type {
            b"NVER" if data.len() >= 4 => {
                out.version = SubReader::new(data).u32_or_default();
            }
            b"DATA" if data.len() >= 24 => {
                out.cell_form = le_u32_at(data, 0);
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
    let mut cur = SubReader::new(data);
    let header = (|| {
        cur.u32().ok()?; // unknown — tracks NVER on the typed form
        cur.u32().ok()?; // unknown — location-ish; not established by the corpus
        let worldspace = cur.u32().ok()?;
        // Interior tiles name their cell; exterior tiles name their grid,
        // y before x. Both are four bytes, so this branch is invisible to a
        // length check — see `parse_navm` for how it was settled.
        let (cell_form, grid) = if worldspace == 0 {
            (Some(cur.u32().ok()?), None)
        } else {
            let y = i32::from(cur.i16().ok()?);
            let x = i32::from(cur.i16().ok()?);
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
        let vertices: Vec<[f32; 3]> = rows(cur.counted(12).ok()?, 12)
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
            rows(cur.counted(16).ok()?, 16).map(decode_nvtr_row).collect();
        // Likewise identical to `NVEX`.
        let external: Vec<NavmExternalConnection> = rows(cur.counted(10).ok()?, 10)
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
        let doors: Vec<NavmDoorTriangle> = rows(cur.counted(10).ok()?, 10)
            .map(|r| NavmDoorTriangle {
                triangle: u16::from_le_bytes([r[0], r[1]]),
                unknown: u32::from_le_bytes([r[2], r[3], r[4], r[5]]),
                door_form_id: u32::from_le_bytes([r[6], r[7], r[8], r[9]]),
            })
            .collect();
        let covers: Vec<u16> = rows(cur.counted(2).ok()?, 2)
            .map(|r| u16::from_le_bytes([r[0], r[1]]))
            .collect();
        let divisor = cur.u32().ok()?;
        if divisor > NVNM_MAX_DIVISOR {
            return None;
        }
        cur.skip(4 * 8).ok()?; // max X/Y distance + min/max XYZ bounds
        for _ in 0..(divisor * divisor) {
            cur.counted(2).ok()?; // per-segment triangle index list
        }
        cur.is_empty().then_some((vertices, triangles, external, doors, covers))
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
        flags: le_u32_at(r, 12).unwrap_or(0),
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
            raw.as_chunks::<2>().0
                .iter()
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
        unknown: le_u32_at(r, 0)?,
        mesh_form: le_u32_at(r, 4)?,
        triangle: u16::from_le_bytes([r[8], r[9]]),
    })
}

#[cfg(test)]
mod navm_tests {
    use super::*;
    
    use crate::esm::records::test_support::sub;

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
        let r = parse_navm(1, &[sub(b"NVDP", row)], &None);
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
        let r = parse_navm(1, &[sub(b"NVCA", data)], &None);
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
        let r = parse_navm(1, &[sub(b"NVEX", row)], &None);
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
            &[verts(3), sub(b"NVTR", tri([0, 1, 2], [0, 0, 0], 0))],
            &None,
        );
        assert!(ok.indices_are_in_range());
        let bad = parse_navm(
            1,
            &[verts(3), sub(b"NVTR", tri([0, 1, 99], [0, 0, 0], 0))],
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
        let r = parse_navm(1, &[sub(b"NVNM", [1, 2, 3, 4])], &None);
        assert_eq!(r.packed_geometry.as_deref(), Some(&[1u8, 2, 3, 4][..]));
        assert!(!r.has_decoded_geometry());
        assert!(r.vertices.is_empty());
        assert_eq!(r.worldspace_form, None);
    }

    #[test]
    fn a_gamebryo_mesh_reports_decoded_geometry() {
        let r = parse_navm(
            1,
            &[verts(3), sub(b"NVTR", tri([0, 1, 2], [0, 0, 0], 0))],
            &None,
        );
        assert!(r.has_decoded_geometry());
        assert!(r.packed_geometry.is_none());
    }

    #[test]
    fn oblivion_style_absence_is_not_an_error() {
        // Oblivion authors zero NAVM records; the model must tolerate a mesh
        // that carries nothing rather than assuming geometry exists.
        let r = parse_navm(1, &[sub(b"NVER", 12u32.to_le_bytes())], &None);
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
                sub(b"NVTR", tri([0, 1, 2], [0, 0, 0], 0)),
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
}
