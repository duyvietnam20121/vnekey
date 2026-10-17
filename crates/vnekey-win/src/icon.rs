//! Vẽ biểu tượng khay hệ thống bằng code: hình tròn có chữ V (tiếng Việt) hoặc E (tiếng Anh).

pub const SIZE: u32 = 32;

const V: [&str; 7] = ["#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#.."];
const E: [&str; 7] = ["#####", "#....", "#....", "####.", "#....", "#....", "#####"];

/// Trả về ảnh RGBA 32x32. `vietnamese = true`: nền xanh chữ V, ngược lại nền xám chữ E.
pub fn render(vietnamese: bool) -> Vec<u8> {
    let (glyph, bg) = if vietnamese { (&V, [46u8, 160, 67]) } else { (&E, [110u8, 110, 110]) };
    let size = SIZE as i32;
    let scale = 3;
    let (gw, gh) = (5 * scale, 7 * scale);
    let (ox, oy) = ((size - gw) / 2, (size - gh) / 2);
    let centre = (size as f32 - 1.0) / 2.0;
    let radius = size as f32 / 2.0 - 0.5;

    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let (dx, dy) = (x as f32 - centre, y as f32 - centre);
            if dx * dx + dy * dy > radius * radius {
                continue; // ngoài hình tròn: trong suốt
            }
            let (gx, gy) = ((x - ox).div_euclid(scale), (y - oy).div_euclid(scale));
            let on = (0..5).contains(&gx)
                && (0..7).contains(&gy)
                && x >= ox
                && y >= oy
                && glyph[gy as usize].as_bytes()[gx as usize] == b'#';
            let [r, g, b] = if on { [255, 255, 255] } else { bg };
            let i = ((y * size + x) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&[r, g, b, 255]);
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(img: &[u8], x: u32, y: u32) -> [u8; 4] {
        let i = ((y * SIZE + x) * 4) as usize;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    #[test]
    fn has_expected_size_and_transparent_corners() {
        let img = render(true);
        assert_eq!(img.len(), (SIZE * SIZE * 4) as usize);
        assert_eq!(px(&img, 0, 0)[3], 0);
        assert_eq!(px(&img, SIZE - 1, SIZE - 1)[3], 0);
    }

    #[test]
    fn colours_differ_between_states() {
        let (v, e) = (render(true), render(false));
        assert_eq!(px(&v, 3, 16), [46, 160, 67, 255]); // nền xanh, bên trái chữ
        assert_eq!(px(&e, 3, 16), [110, 110, 110, 255]); // nền xám
        assert_ne!(v, e);
    }

    #[test]
    fn glyph_pixels_are_white() {
        // Hàng trên cùng của chữ E là một vạch ngang đầy: các điểm giữa phải trắng.
        let e = render(false);
        let (ox, oy) = ((SIZE as i32 - 15) / 2, (SIZE as i32 - 21) / 2);
        assert_eq!(px(&e, (ox + 7) as u32, (oy + 1) as u32), [255, 255, 255, 255]);
    }
}
