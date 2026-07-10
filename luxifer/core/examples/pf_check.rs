use luxifer_core::pattern_fill::{fill_pattern, FillParams, Pattern};
fn main() {
    let sq: Vec<(f64, f64)> = vec![(5.0, 5.0), (45.0, 5.0), (45.0, 32.0), (5.0, 32.0)];
    let p = FillParams {
        pattern: Pattern::Hex,
        gap_x: 0.8,
        gap_y: 0.8,
        angle_deg: 0.0,
        size: 2.5,
    };
    let out = fill_pattern(&[sq.clone()], &p);
    let sc = 8.0;
    let mut img = image::GrayImage::from_pixel(400, 300, image::Luma([245]));
    let mut line = |a: (f64, f64), b: (f64, f64), v: u8| {
        let steps = (((b.0 - a.0).abs().max((b.1 - a.1).abs())) * sc) as usize + 1;
        for s in 0..=steps {
            let t = s as f64 / steps as f64;
            let (x, y) = (
                ((a.0 + (b.0 - a.0) * t) * sc) as i64,
                ((a.1 + (b.1 - a.1) * t) * sc) as i64,
            );
            if x >= 0 && y >= 0 && x < 400 && y < 300 {
                img.put_pixel(x as u32, y as u32, image::Luma([v]));
            }
        }
    };
    for i in 0..sq.len() {
        line(sq[i], sq[(i + 1) % sq.len()], 0);
    }
    for (pts, closed) in &out {
        let n = pts.len();
        let m = if *closed { n } else { n - 1 };
        for i in 0..m {
            line(pts[i], pts[(i + 1) % n], 80);
        }
    }
    img.save(std::env::args().nth(1).unwrap()).unwrap();
    println!("{} Konturen", out.len());
}
