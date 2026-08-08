//! SF D2: tolerant re-implementation of the BSGeometryMeshData body to find
//! where FaceMeshes `.mesh` files diverge from the shipped parser.
use byroredux_bsa::Ba2Archive;
use std::collections::BTreeMap;

struct R<'a> { b: &'a [u8], o: usize }
impl<'a> R<'a> {
    fn u32(&mut self) -> Option<u32> {
        let v = self.b.get(self.o..self.o + 4)?;
        self.o += 4;
        Some(u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
    }
    fn f32(&mut self) -> Option<f32> { self.u32().map(f32::from_bits) }
    fn skip(&mut self, n: usize) -> Option<()> {
        if self.o + n > self.b.len() { return None; }
        self.o += n; Some(())
    }
}

fn probe(b: &[u8]) -> Result<(u32, usize, bool, bool, bool), String> {
    let mut r = R { b, o: 0 };
    let ver = r.u32().ok_or("ver")?;
    let nti = r.u32().ok_or("nti")?;
    r.skip((nti as usize / 3) * 6).ok_or("tris")?;
    let scale = r.f32().ok_or("scale")?;
    if scale <= 0.0 { return Ok((ver, 0, false, true, true)); }
    let wpv = r.u32().ok_or("wpv")?;
    let nv = r.u32().ok_or("nv")?;
    r.skip(nv as usize * 6).ok_or("verts")?;
    let mut counts = Vec::new();
    for label in ["uv1", "uv2", "colors", "normals", "tangents"] {
        let n = r.u32().ok_or(label)?;
        r.skip(n as usize * 4).ok_or(label)?;
        counts.push(n);
    }
    let ntw = r.u32().ok_or("ntw")?;
    if wpv != 0 { r.skip((ntw as usize / wpv as usize) * wpv as usize * 4).ok_or("weights")?; }
    let nlods = r.u32().ok_or("nlods")?;
    for _ in 0..nlods {
        let n = r.u32().ok_or("lodn")?;
        r.skip((n as usize / 3) * 6).ok_or("lodtri")?;
    }
    let after_lods = r.o;
    let ends_after_lods = after_lods == b.len();
    // channel sanity
    let sane = counts.iter().all(|&c| c == 0 || c == nv);
    let _ = after_lods; if ends_after_lods { return Ok((ver, nv as usize, true, false, sane)); }
    let nm = r.u32().ok_or("nmeshlets")?;
    r.skip(nm as usize * 16).ok_or("meshlets")?;
    let nc = r.u32().ok_or("ncull")?;
    r.skip(nc as usize * 24).ok_or("cull")?;
    if r.o != b.len() { return Err(format!("trailing {} bytes", b.len() as i64 - r.o as i64)); }
    Ok((ver, nv as usize, false, false, sane))
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ar = Ba2Archive::open(&a[1]).expect("open");
    let stride: usize = a.get(2).map(|s| s.parse().unwrap()).unwrap_or(1);
    let names: Vec<String> = ar.list_files().into_iter()
        .filter(|n| n.to_ascii_lowercase().ends_with(".mesh")).map(|s| s.to_string()).collect();
    println!("total .mesh entries: {}", names.len());
    let mut ends_after_lods = 0usize; let mut has_meshlets = 0usize;
    let mut errs: BTreeMap<String, usize> = BTreeMap::new();
    let mut insane = 0usize; let mut n = 0usize; let mut sentinel = 0usize;
    for name in names.iter().step_by(stride) {
        let Ok(bytes) = ar.extract(name) else { continue };
        n += 1;
        match probe(&bytes) {
            Ok((_v, _nv, short, sent, sane)) => {
                if sent { sentinel += 1; continue; }
                if !sane { insane += 1; }
                if short { ends_after_lods += 1; } else { has_meshlets += 1; }
            }
            Err(e) => { *errs.entry(e).or_default() += 1; }
        }
    }
    println!("{} sampled={} ends_at_lods(no meshlet tail)={} full_tail={} sentinel={} ragged={}",
        a[1], n, ends_after_lods, has_meshlets, sentinel, insane);
    println!("errors: {:?}", errs);
}
