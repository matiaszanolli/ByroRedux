//! D9 probe: (a) list any .bgsm/.bgem in Starfield archives, (b) sample the
//! Starfield BSLSP stub names that carry a .bgsm suffix.
use byroredux_bsa::Ba2Archive;
use byroredux_nif::blocks::shader::BSLightingShaderProperty;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args[0].clone();
    if mode == "list" {
        for path in &args[1..] {
            let Ok(a) = Ba2Archive::open(path) else {
                eprintln!("skip {path}");
                continue;
            };
            let files = a.list_files();
            let n: Vec<String> = files
                .iter()
                .map(|s| s.to_string())
                .filter(|f: &String| {
                    let l = f.to_ascii_lowercase();
                    l.ends_with(".bgsm") || l.ends_with(".bgem") || l.ends_with(".mat")
                })
                .collect();
            println!("{path}: total={} bgsm/bgem/mat={}", files.len(), n.len());
            for f in n.iter().take(5) {
                println!("   {f}");
            }
        }
        return;
    }
    // sample mode
    let mut samples: Vec<String> = Vec::new();
    let mut n = 0u64;
    for path in &args[1..] {
        let Ok(a) = Ba2Archive::open(path) else {
            continue;
        };
        let names: Vec<String> = a
            .list_files()
            .into_iter()
            .filter(|f| f.to_ascii_lowercase().ends_with(".nif"))
            .map(|s| s.to_string())
            .take(3000)
            .collect();
        for name in names {
            let Ok(bytes) = a.extract(&name) else {
                continue;
            };
            let Ok(scene) = byroredux_nif::parse_nif(&bytes) else {
                continue;
            };
            for i in 0..scene.blocks.len() {
                if let Some(sp) = scene.get_as::<BSLightingShaderProperty>(i) {
                    if !sp.material_reference {
                        continue;
                    }
                    let nm = sp.net.name.as_deref().unwrap_or("");
                    if nm.to_ascii_lowercase().ends_with(".bgsm")
                        || nm.to_ascii_lowercase().ends_with(".bgem")
                    {
                        n += 1;
                        if samples.len() < 15 {
                            samples.push(format!("{name} :: {nm}"));
                        }
                    }
                }
            }
        }
    }
    println!("bgsm/bgem-suffixed Starfield stubs: {n}");
    for s in samples {
        println!("  {s}");
    }
}
