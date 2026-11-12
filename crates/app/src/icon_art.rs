// App icon artwork, shared by build.rs (the .ico embedded in the exe, used by the taskbar,
// Explorer and shortcuts) and the app (the live window icon). Plain Rust, no dependencies:
// a rounded square in the app's accent gradient with the dashboard glyph in white.

/// RGBA pixels (row-major, top-down) of the icon at `n`×`n`.
#[allow(dead_code)]
pub fn icon_rgba(n: u32) -> Vec<u8> {
    const SS: u32 = 4; // supersampling per axis, for smooth edges at every size
    let nf = n as f32;
    let radius = nf * 0.23;
    let (a, b) = ([0x7c as f32, 0x8c as f32, 0xff as f32], [0x4a as f32, 0xde as f32, 0x80 as f32]);
    // Dashboard glyph (same shapes as the Overview icon, on its 24-unit grid, spanning 4..20).
    let tiles: [(f32, f32, f32, f32); 4] = [(4.0, 4.0, 11.0, 11.0), (13.0, 4.0, 20.0, 8.0), (13.0, 10.0, 20.0, 20.0), (4.0, 13.0, 11.0, 20.0)];
    // The glyph fills the middle 56% of the icon.
    let inset = nf * 0.22;
    let unit = (nf - 2.0 * inset) / 16.0;
    let tile_r = (unit * 1.1).max(0.6);

    let in_round_rect = |x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32| {
        let dx = (x0 + r - x).max(x - (x1 - r)).max(0.0);
        let dy = (y0 + r - y).max(y - (y1 - r)).max(0.0);
        x >= x0 && x <= x1 && y >= y0 && y <= y1 && dx * dx + dy * dy <= r * r
    };

    let mut out = Vec::with_capacity((n * n * 4) as usize);
    for py in 0..n {
        for px in 0..n {
            let (mut cover, mut white) = (0u32, 0u32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = px as f32 + (sx as f32 + 0.5) / SS as f32;
                    let y = py as f32 + (sy as f32 + 0.5) / SS as f32;
                    if !in_round_rect(x, y, 0.0, 0.0, nf, nf, radius) {
                        continue;
                    }
                    cover += 1;
                    let gx = (x - inset) / unit + 4.0;
                    let gy = (y - inset) / unit + 4.0;
                    if tiles.iter().any(|&(x0, y0, x1, y1)| in_round_rect(gx * unit, gy * unit, x0 * unit, y0 * unit, x1 * unit, y1 * unit, tile_r)) {
                        white += 1;
                    }
                }
            }
            let total = (SS * SS) as f32;
            let alpha = cover as f32 / total;
            let w = if cover > 0 { white as f32 / cover as f32 } else { 0.0 };
            let t = (px + py) as f32 / (2 * n.max(2) - 2) as f32;
            for c in 0..3 {
                let base = a[c] + (b[c] - a[c]) * t;
                out.push((base + (255.0 - base) * w).round() as u8);
            }
            out.push((alpha * 255.0).round() as u8);
        }
    }
    out
}

/// A Windows .ico holding the icon at the usual sizes (32-bit BMP entries).
#[allow(dead_code)]
pub fn ico_bytes(sizes: &[u32]) -> Vec<u8> {
    let mut images = Vec::new();
    for &n in sizes {
        let rgba = icon_rgba(n);
        let mut img = Vec::new();
        // BITMAPINFOHEADER; height is doubled to include the (empty) AND mask.
        img.extend_from_slice(&40u32.to_le_bytes());
        img.extend_from_slice(&(n as i32).to_le_bytes());
        img.extend_from_slice(&((2 * n) as i32).to_le_bytes());
        img.extend_from_slice(&1u16.to_le_bytes());
        img.extend_from_slice(&32u16.to_le_bytes());
        img.extend_from_slice(&[0u8; 24]);
        // Pixels bottom-up, BGRA.
        for y in (0..n).rev() {
            for x in 0..n {
                let i = ((y * n + x) * 4) as usize;
                img.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
            }
        }
        // AND mask: 1 bit per pixel, rows padded to 4 bytes, all zero (alpha does the work).
        let row = n.div_ceil(32) * 4;
        img.extend(std::iter::repeat_n(0u8, (row * n) as usize));
        images.push((n, img));
    }
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (n, img) in &images {
        let dim = if *n >= 256 { 0 } else { *n as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(img.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += img.len() as u32;
    }
    for (_, img) in images {
        out.extend_from_slice(&img);
    }
    out
}
