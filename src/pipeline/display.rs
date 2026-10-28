use super::tracker::{Track, TrackState};

const COLOR_SEARCH: u32 = 0x00FF_FF00; // yellow
const COLOR_LOCKED: u32 = 0x0000_FF00; // green
const COLOR_LOST: u32 = 0x00FF_0000; // red
const COLOR_OVERLAP: u32 = 0x00FF_8800; // orange

// 3x5 bitmap font for digits, one row per byte, bit 2 = left column
const GLYPH_W: usize = 3;
const GLYPH_H: usize = 5;
const SCALE: usize = 2;
const FONT: [[u8; GLYPH_H]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111], // 0
    [0b010, 0b110, 0b010, 0b010, 0b111], // 1
    [0b111, 0b001, 0b111, 0b100, 0b111], // 2
    [0b111, 0b001, 0b111, 0b001, 0b111], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b111, 0b001, 0b111], // 5
    [0b111, 0b100, 0b111, 0b101, 0b111], // 6
    [0b111, 0b001, 0b001, 0b001, 0b001], // 7
    [0b111, 0b101, 0b111, 0b101, 0b111], // 8
    [0b111, 0b101, 0b111, 0b001, 0b111], // 9
];

/// Draws each track: box centred on the smoothed position, centre cross, id, color by state.
pub fn draw_tracks(buffer: &mut [u32], width: usize, height: usize, tracks: &[Track]) {
    for track in tracks {
        let color = state_color(track.state);
        if let Some((x0, y0, x1, y1)) = track_bounds(track, width, height) {
            draw_rect(buffer, width, (x0, y0), (x1, y1), color);
            // id above the box, or inside if there is no room
            let label_y = y0.checked_sub(GLYPH_H * SCALE + 2).unwrap_or(y0 + 2);
            draw_number(buffer, width, height, x0, label_y, track.id, color);
        }
        draw_cross(buffer, width, height, track.x, track.y, color);
    }
}

fn state_color(state: TrackState) -> u32 {
    match state {
        TrackState::Recherche => COLOR_SEARCH,
        TrackState::Verrouille { .. } => COLOR_LOCKED,
        TrackState::Perdu { .. } => COLOR_LOST,
        TrackState::Recouvrement { .. } => COLOR_OVERLAP,
    }
}

/// Box clamped to the image, `None` if it is entirely outside.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn track_bounds(t: &Track, width: usize, height: usize) -> Option<(usize, usize, usize, usize)> {
    let x0 = (t.x - t.half_w).max(0.0);
    let y0 = (t.y - t.half_h).max(0.0);
    let x1 = (t.x + t.half_w).min(width as f32 - 1.0);
    let y1 = (t.y + t.half_h).min(height as f32 - 1.0);
    if x0 > x1 || y0 > y1 {
        return None;
    }
    Some((x0 as usize, y0 as usize, x1 as usize, y1 as usize))
}

/// 1-px hollow rectangle, inclusive bounds, guaranteed inside the image.
fn draw_rect(
    buffer: &mut [u32],
    width: usize,
    (x0, y0): (usize, usize),
    (x1, y1): (usize, usize),
    color: u32,
) {
    let top = y0 * width;
    let bottom = y1 * width;

    buffer[top + x0..=top + x1].fill(color);
    buffer[bottom + x0..=bottom + x1].fill(color);

    buffer[top + x0..=bottom + x0]
        .iter_mut()
        .step_by(width)
        .for_each(|px| *px = color);
    buffer[top + x1..=bottom + x1]
        .iter_mut()
        .step_by(width)
        .for_each(|px| *px = color);
}

/// Bounds-checked pixel write (text and crosses can go past the edges).
fn set_pixel(buffer: &mut [u32], width: usize, height: usize, x: usize, y: usize, color: u32) {
    if x < width && y < height {
        buffer[y * width + x] = color;
    }
}

/// Centre cross. A negative centre is skipped; a huge one saturates to `usize::MAX`
/// (float -> int casts saturate), so the additions must saturate too.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn draw_cross(buffer: &mut [u32], width: usize, height: usize, x: f32, y: f32, color: u32) {
    if x < 0.0 || y < 0.0 {
        return;
    }
    let (cx, cy) = (x as usize, y as usize);
    for d in 0..=3 {
        set_pixel(buffer, width, height, cx.saturating_add(d), cy, color);
        set_pixel(buffer, width, height, cx, cy.saturating_add(d), color);
        if let Some(l) = cx.checked_sub(d) {
            set_pixel(buffer, width, height, l, cy, color);
        }
        if let Some(u) = cy.checked_sub(d) {
            set_pixel(buffer, width, height, cx, u, color);
        }
    }
}

/// Writes a number without allocating: digits go into a stack array.
fn draw_number(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    mut n: u64,
    color: u32,
) {
    let mut digits = [0u8; 20]; // u64::MAX has 20 digits
    let mut len = 0;
    loop {
        digits[len] = (n % 10) as u8; // always < 10
        len += 1;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    for (k, &d) in digits[..len].iter().rev().enumerate() {
        let gx = x + k * (GLYPH_W + 1) * SCALE;
        draw_glyph(buffer, width, height, gx, y, d, color);
    }
}

fn draw_glyph(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    digit: u8,
    color: u32,
) {
    for (row, bits) in FONT[usize::from(digit)].iter().enumerate() {
        for col in 0..GLYPH_W {
            if bits & (1 << (GLYPH_W - 1 - col)) != 0 {
                for sy in 0..SCALE {
                    for sx in 0..SCALE {
                        set_pixel(
                            buffer,
                            width,
                            height,
                            x + col * SCALE + sx,
                            y + row * SCALE + sy,
                            color,
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::blobs::Region;
    use crate::pipeline::tracker::Tracker;

    const W: usize = 64;
    const H: usize = 48;

    /// A real 10x10 track built through the tracker, then moved to (x, y)
    fn track_at(x: f32, y: f32) -> Track {
        let mut tracker = Tracker::new();
        tracker.update(&[Region {
            min_x: 10,
            max_x: 19,
            min_y: 10,
            max_y: 19,
            area: 100,
            cx: 15.0,
            cy: 15.0,
        }]);
        let mut t = tracker.tracks()[0];
        t.x = x;
        t.y = y;
        t
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn track_outside_the_image_does_not_panic() {
        let mut buffer = vec![0u32; W * H];
        for (x, y) in [
            (-1.0, -1.0),
            (-100.0, 20.0),
            (W as f32 + 50.0, 20.0),
            (20.0, H as f32 + 50.0),
            (f32::MAX, f32::MAX),
        ] {
            draw_tracks(&mut buffer, W, H, &[track_at(x, y)]);
        }
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn box_entirely_outside_has_no_bounds() {
        assert_eq!(track_bounds(&track_at(-100.0, 20.0), W, H), None);
        assert_eq!(track_bounds(&track_at(W as f32 + 50.0, 20.0), W, H), None);
    }

    #[test]
    fn box_partly_outside_is_clamped() {
        // Centre at -1 px, half size 5: the box is clamped to x in [0, 4]
        assert_eq!(
            track_bounds(&track_at(-1.0, 20.0), W, H),
            Some((0, 15, 4, 25))
        );
    }
}
