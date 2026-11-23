// (cx, cy, r) in a 0..100 box; blobs merge metaball-style into a splatter.
pub const BLOBS: &[(f32, f32, f32)] = &[
    (50.0, 52.0, 17.0),
    (36.0, 44.0, 10.0),
    (62.0, 40.0, 11.0),
    (58.0, 66.0, 12.0),
    (40.0, 64.0, 9.0),
    (22.0, 30.0, 5.0),
    (78.0, 24.0, 4.5),
    (82.0, 60.0, 6.0),
    (20.0, 72.0, 4.0),
    (66.0, 84.0, 4.5),
    (34.0, 86.0, 3.0),
    (88.0, 40.0, 2.5),
    (14.0, 50.0, 3.0),
    (50.0, 14.0, 3.5),
];

fn splat(x: f32, y: f32) -> bool {
    BLOBS.iter().map(|&(cx, cy, r)| (r * r / ((x - cx).powi(2) + (y - cy).powi(2)).max(0.01)).powi(2)).sum::<f32>() >= 1.0
}

fn in_tile(x: f32, y: f32) -> bool {
    let (r, lo, hi) = (22.0f32, 0.0f32, 100.0f32);
    let qx = x.clamp(lo + r, hi - r);
    let qy = y.clamp(lo + r, hi - r);
    (x - qx).powi(2) + (y - qy).powi(2) <= r * r
}

/// Rounded tile with a splatter; `light` = white tile / black splat, else the reverse.
pub fn rgba(n: u32, light: bool) -> Vec<u8> {
    let (bg, fg) = if light { (255u8, 0u8) } else { (0u8, 255u8) };
    let ss = 3;
    let mut out = Vec::with_capacity((n * n * 4) as usize);
    for py in 0..n {
        for px in 0..n {
            let (mut tile, mut ink) = (0u32, 0u32);
            for sy in 0..ss {
                for sx in 0..ss {
                    let x = (px as f32 + (sx as f32 + 0.5) / ss as f32) * 100.0 / n as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / ss as f32) * 100.0 / n as f32;
                    if in_tile(x, y) {
                        tile += 1;
                        if splat(x, y) {
                            ink += 1;
                        }
                    }
                }
            }
            let total = (ss * ss) as f32;
            let c = if tile == 0 { 0.0 } else { (tile - ink) as f32 * bg as f32 / tile as f32 + ink as f32 * fg as f32 / tile as f32 };
            out.extend_from_slice(&[c as u8, c as u8, c as u8, (tile as f32 / total * 255.0) as u8]);
        }
    }
    out
}

/// True when Windows is using the light theme for apps.
pub fn system_light(value: &str) -> bool {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("reg")
        .args(["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "/v", value])
        .creation_flags(0x0800_0000)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("0x1"))
        .unwrap_or(false)
}
