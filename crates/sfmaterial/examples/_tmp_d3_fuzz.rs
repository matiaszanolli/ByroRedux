use byroredux_sfmaterial::{ChunkType, ComponentDatabaseFile};

fn hdr(chunk_count_incl_beth: u32) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&0x48544542u32.to_le_bytes());
    b.extend_from_slice(&8u32.to_le_bytes());
    b.extend_from_slice(&4u32.to_le_bytes());
    b.extend_from_slice(&chunk_count_incl_beth.to_le_bytes());
    b
}
fn chunk(b: &mut Vec<u8>, t: ChunkType, payload: &[u8]) {
    b.extend_from_slice(&(t as u32).to_le_bytes());
    b.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    b.extend_from_slice(payload);
}

fn main() {
    let which = std::env::args().nth(1).unwrap_or_default();

    if which == "negcount" {
        // BETH + STRT + TYPE(0) + LIST(elem=Int32, count=-1)
        let mut b = hdr(4);
        chunk(&mut b, ChunkType::Strt, b"");
        chunk(&mut b, ChunkType::Type, &0u32.to_le_bytes());
        let mut list = Vec::new();
        list.extend_from_slice(&(0xFFFFFF0Cu32 as i32).to_le_bytes()); // Int32 builtin
        list.extend_from_slice(&(-1i32).to_le_bytes()); // count = -1
        chunk(&mut b, ChunkType::List, &list);
        println!("[negcount] calling parse on {} bytes ...", b.len());
        let r = ComponentDatabaseFile::parse(&b);
        println!("[negcount] result = {:?}", r.map(|c| c.instances.len()));
    }

    if which == "bigcount" {
        let mut b = hdr(4);
        chunk(&mut b, ChunkType::Strt, b"");
        chunk(&mut b, ChunkType::Type, &0u32.to_le_bytes());
        let mut list = Vec::new();
        list.extend_from_slice(&(0xFFFFFF0Cu32 as i32).to_le_bytes());
        list.extend_from_slice(&0x3FFF_FFFFi32.to_le_bytes()); // ~1.07e9 elements
        chunk(&mut b, ChunkType::List, &list);
        println!("[bigcount] calling parse (payload declares 1.07e9 Values, ~34 GB) ...");
        let r = ComponentDatabaseFile::parse(&b);
        println!("[bigcount] result = {:?}", r.map(|c| c.instances.len()));
    }

    if which == "mapnegcount" {
        let mut b = hdr(4);
        chunk(&mut b, ChunkType::Strt, b"");
        chunk(&mut b, ChunkType::Type, &0u32.to_le_bytes());
        let mut m = Vec::new();
        m.extend_from_slice(&(0xFFFFFF0Cu32 as i32).to_le_bytes());
        m.extend_from_slice(&(0xFFFFFF0Cu32 as i32).to_le_bytes());
        m.extend_from_slice(&(-1i32).to_le_bytes());
        chunk(&mut b, ChunkType::Mapc, &m);
        println!("[mapnegcount] calling parse ...");
        let r = ComponentDatabaseFile::parse(&b);
        println!("[mapnegcount] result = {:?}", r.map(|c| c.instances.len()));
    }

    if which == "probe_no_strt" {
        // Header + chunk table well-formed but no STRT/TYPE at all.
        let mut b = hdr(2);
        chunk(&mut b, ChunkType::Objt, b"");
        println!(
            "[probe_no_strt] probe_header = {:?}",
            ComponentDatabaseFile::probe_header(&b)
        );
        println!(
            "[probe_no_strt] peek_magic  = {}",
            ComponentDatabaseFile::peek_magic(&b)
        );
        println!(
            "[probe_no_strt] full parse  = {:?}",
            ComponentDatabaseFile::parse(&b).is_ok()
        );
    }

    if which == "probe_truncated" {
        // Claim 1 chunk (i.e. BETH only) -> chunk table is empty; probe passes.
        let b = hdr(1);
        println!(
            "[probe_truncated] probe_header = {:?}",
            ComponentDatabaseFile::probe_header(&b)
        );
    }
}
