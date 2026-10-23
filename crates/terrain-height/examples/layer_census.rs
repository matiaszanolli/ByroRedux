//! Corpus check for terrain height generation (#6195) against real game
//! data: for every (diffuse, normal) terrain layer pair, integrate the normal
//! map and report the green convention the integrability test picked, how
//! decisively, the relief, and the correlation between the integrated height
//! and the diffuse's high-passed brightness (raised stones are lit, cracks
//! are in cavity shadow, so a correctly signed height correlates positively).
//!
//! ```sh
//! cargo run --release -p byroredux-terrain-height --example layer_census -- \
//!     <archives-file> <pairs-file> [png-out-dir]
//! ```
//!
//! `archives-file`: one BSA/BA2 path per line. `pairs-file`: one
//! `diffuse<TAB>normal` archive key pair per line.

use byroredux_menuxml::tex::Rgba8;
use byroredux_terrain_height as th;

enum Archive {
    Bsa(byroredux_bsa::BsaArchive),
    Ba2(byroredux_bsa::Ba2Archive),
}

impl Archive {
    fn extract(&self, path: &str) -> Option<Vec<u8>> {
        match self {
            Archive::Bsa(a) => a.extract(path).ok(),
            Archive::Ba2(a) => a.extract(path).ok(),
        }
    }
}

fn resample(field: &th::HeightField, w: usize, h: usize) -> Vec<f32> {
    (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            field.data[(y * field.height / h) * field.width + x * field.width / w]
        })
        .collect()
}

fn view(img: &Rgba8) -> th::Rgba<'_> {
    th::Rgba {
        width: img.width as usize,
        height: img.height as usize,
        pixels: &img.pixels,
    }
}

fn correlation(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len() as f32;
    let (ma, mb) = (a.iter().sum::<f32>() / n, b.iter().sum::<f32>() / n);
    let (mut num, mut da, mut db) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        num += (x - ma) * (y - mb);
        da += (x - ma) * (x - ma);
        db += (y - mb) * (y - mb);
    }
    num / (da.sqrt() * db.sqrt()).max(f32::MIN_POSITIVE)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let archives: Vec<Archive> = std::fs::read_to_string(&args[1])
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|path| {
            let magic = std::fs::read(path).ok().map(|b| b[..4].to_vec())?;
            if magic == b"BTDX" {
                byroredux_bsa::Ba2Archive::open(path).ok().map(Archive::Ba2)
            } else {
                byroredux_bsa::BsaArchive::open(path).ok().map(Archive::Bsa)
            }
        })
        .collect();
    let png_dir = args.get(3).map(std::path::PathBuf::from);
    let extract = |key: &str| archives.iter().rev().find_map(|a| a.extract(key));

    let (mut up, mut down, mut positive, mut total) = (0, 0, 0, 0);
    let mut decisiveness = Vec::new();
    for line in std::fs::read_to_string(&args[2]).unwrap().lines() {
        let Some((diffuse_key, normal_key)) = line.split_once('\t') else {
            continue;
        };
        let (Some(nb), Some(db)) = (extract(normal_key), extract(diffuse_key)) else {
            println!("MISSING  {normal_key}");
            continue;
        };
        let (Some(normal), Some(diffuse)) = (Rgba8::decode_dds(&nb), Rgba8::decode_dds(&db)) else {
            println!("UNDECODED {normal_key}");
            continue;
        };
        let green = match std::env::var("GREEN").as_deref() {
            Ok("down") => Some(th::GreenConvention::YDown),
            Ok("up") => Some(th::GreenConvention::YUp),
            _ => None,
        };
        let generated = th::from_normal_map(&view(&normal), green);
        let height = Rgba8::decode_dds(&generated.dds)
            .map(|image| th::HeightField::from_red_channel(image.width as usize, image.height as usize, &image.pixels))
            .unwrap();
        let bright = th::brightness::highpass_luminance(&view(&diffuse));
        let r = correlation(&resample(&height, 256, 256), &resample(&bright, 256, 256));
        let meta = generated.meta;
        match meta.detected_green {
            Some(th::GreenConvention::YUp) => up += 1,
            _ => down += 1,
        }
        total += 1;
        if r > 0.0 {
            positive += 1;
        }
        decisiveness.push(meta.convention_decisiveness.unwrap());
        println!(
            "{:<6} decisive {:>6.2}  relief {:>7.1} tx  corr {:+.2}  {}x{}  {normal_key}",
            format!("{:?}", meta.green.unwrap()),
            meta.convention_decisiveness.unwrap(),
            meta.relief,
            r,
            normal.width,
            normal.height,
        );
        if let Some(dir) = &png_dir {
            std::fs::create_dir_all(dir).unwrap();
            let stem = normal_key
                .rsplit('\\')
                .next()
                .unwrap()
                .trim_end_matches(".dds");
            let (w, h) = (height.width as u32, height.height as u32);
            let px: Vec<u8> = height.data.iter().map(|v| (v * 255.0) as u8).collect();
            image::GrayImage::from_raw(w, h, px)
                .unwrap()
                .save(dir.join(format!("{stem}_height.png")))
                .unwrap();
            image::RgbaImage::from_raw(diffuse.width, diffuse.height, diffuse.pixels.clone())
                .unwrap()
                .save(dir.join(format!("{stem}_diffuse.png")))
                .unwrap();
        }
    }
    decisiveness.sort_by(f32::total_cmp);
    let median = decisiveness
        .get(decisiveness.len() / 2)
        .copied()
        .unwrap_or(0.0);
    let low = decisiveness.iter().filter(|&&d| d < 1.5).count();
    println!(
        "SUMMARY layers {total}: green YUp {up} / YDown {down}; decisiveness median {median:.2}, {low} below 1.5; height~brightness corr > 0 on {positive}/{total}"
    );
}
