use byroredux_bsa::Ba2Archive;
use byroredux_nif::header::NifHeader;
use std::collections::BTreeMap;
fn main(){
  let p="/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/Starfield - Meshes02.ba2";
  let a=Ba2Archive::open(p).unwrap();
  let all:Vec<String>=a.list_files().into_iter().map(|s|s.to_string()).collect();
  let mut ext:BTreeMap<String,usize>=BTreeMap::new();
  for f in &all { let e=f.rsplit('.').next().unwrap_or("").to_lowercase(); *ext.entry(e).or_insert(0)+=1; }
  println!("total entries={}",all.len());
  for (k,v) in &ext { println!("EXT {v}\t{k}"); }
  let nifs:Vec<&String>=all.iter().filter(|f|byroredux_nif::corpus::is_nif_entry(f)).collect();
  let mut hist:BTreeMap<String,usize>=BTreeMap::new();
  for f in nifs.iter().take(4000) {
     let Ok(d)=a.extract(f) else {continue};
     let Ok((h,_))=NifHeader::parse(&d) else {continue};
     for i in 0..h.num_blocks as usize { if let Some(t)=h.block_type_name(i){ *hist.entry(t.to_string()).or_insert(0)+=1; } }
  }
  let mut v:Vec<_>=hist.into_iter().collect(); v.sort_by(|a,b|b.1.cmp(&a.1));
  for (k,c) in v.iter().take(20){ println!("M02HIST {c}\t{k}"); }
  for f in nifs.iter().take(5){ println!("SAMPLE {f}"); }
}
