//! Throughput of the image path: `Image::decode` and `image::draw` per mode, on BMPs in
//! bench/images/ (run bench/fetch.sh first). No harness: `cargo bench --bench image`.
//! Reports source megapixels per second (median of runs, at least 0.5 s each) and writes the
//! braille 1-bit grids to bench/out/ so bench/compare.py can score them.
use halftone::image::{self, Dither, Image, Mode, Opts};
use halftone::{Buffer, Rect};
use std::time::{Duration, Instant};

const TERM: (u16, u16) = (200, 60);

fn median_secs(mut f: impl FnMut()) -> f64 {
    f(); // warm up
    let (mut t, start) = (Vec::new(), Instant::now());
    while t.len() < 7 || start.elapsed() < Duration::from_millis(500) {
        let s = Instant::now();
        f();
        t.push(s.elapsed().as_secs_f64());
    }
    t.sort_by(|a, b| a.partial_cmp(b).unwrap());
    t[t.len() / 2]
}

fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("bench");
    let mut files: Vec<_> = std::fs::read_dir(dir.join("images"))
        .expect("bench/images missing: run bench/fetch.sh")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "bmp"))
        .collect();
    files.sort();
    std::fs::create_dir_all(dir.join("out")).unwrap();
    println!("image,stage,mpix_per_s");
    for p in files {
        let name = p.file_stem().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(&p).unwrap();
        let img = Image::decode(&bytes).unwrap();
        let mp = (img.w as f64 * img.h as f64) / 1e6;
        let s = median_secs(|| drop(std::hint::black_box(Image::decode(&bytes).unwrap())));
        println!("{name},decode,{:.1}", mp / s);

        let modes = [
            ("half", Mode::Half, Dither::Bayer),
            ("quad", Mode::Quad, Dither::Bayer),
            ("braille_none", Mode::Braille, Dither::None),
            ("braille_bayer", Mode::Braille, Dither::Bayer),
            ("braille_floyd", Mode::Braille, Dither::Floyd),
            ("blocks_bayer", Mode::Blocks, Dither::Bayer),
            ("ascii_floyd", Mode::Ascii, Dither::Floyd),
        ];
        for (label, mode, dither) in modes {
            let mut b = Buffer::new(TERM.0, TERM.1);
            let o = Opts { mode, dither, contrast: 1.0 };
            let r = Rect::new(0, 0, TERM.0, TERM.1);
            let s = median_secs(|| {
                std::hint::black_box(image::draw(&mut b, r, &img, o));
            });
            println!("{name},{label},{:.1}", mp / s);
            if mode == Mode::Braille {
                let out = image::draw(&mut b, r, &img, o);
                dump_braille(&b, out, &dir.join("out").join(format!("{name}_{label}.pgm")));
            }
        }
    }
}

/// Writes the braille cells back out as a 1-bit grid (P5, 0/255), 2x4 dots per cell.
fn dump_braille(b: &Buffer, out: Rect, path: &std::path::Path) {
    let (w, h) = (out.w as usize * 2, out.h as usize * 4);
    let mut px = vec![0u8; w * h];
    for cy in 0..out.h {
        for cx in 0..out.w {
            let ch = b.get(out.x + cx, out.y + cy).ch as u32;
            let bits = if (0x2800..0x2900).contains(&ch) { (ch - 0x2800) as u8 } else { 0 };
            // Braille dot order: (0,0)=1 (0,1)=2 (0,2)=4 (1,0)=8 (1,1)=16 (1,2)=32 (0,3)=64 (1,3)=128.
            const BIT: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];
            for dx in 0..2 {
                for dy in 0..4 {
                    if bits & BIT[dx][dy] != 0 {
                        px[(cy as usize * 4 + dy) * w + cx as usize * 2 + dx] = 255;
                    }
                }
            }
        }
    }
    let mut f = format!("P5\n{w} {h}\n255\n").into_bytes();
    f.extend(px);
    std::fs::write(path, f).unwrap();
}
