//! Connected components: turns the change mask into regions.
//!
//! The mask is consumed by the flood fill, so every pixel is cleared at the end.
//! The queue and the mask are passed in, so nothing is allocated per frame.

use std::collections::VecDeque;

/// Bounding box, area and centroid of a connected component.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Region {
    pub min_x: usize,
    pub max_x: usize,
    pub min_y: usize,
    pub max_y: usize,
    /// Number of pixels in the component.
    pub area: usize,
    /// Centroid: mean of the pixel coordinates.
    pub cx: f32,
    pub cy: f32,
}

/// Finds the 8-connected components of the mask and stores those with
/// `area >= min_area` in `regions`. The mask is consumed: every pixel ends up `false`.
///
/// # Panics
/// Panics if `width` or `height` does not fit in a `u16` (the caller must check it).
pub fn find_regions(
    mask: &mut [bool],
    width: usize,
    height: usize,
    min_area: usize,
    queue: &mut VecDeque<(u16, u16)>,
    regions: &mut Vec<Region>,
) {
    debug_assert!(u16::try_from(width).is_ok() && u16::try_from(height).is_ok());
    debug_assert_eq!(mask.len(), width * height);

    regions.clear();

    for start in 0..mask.len() {
        if !mask[start] {
            continue;
        }
        mask[start] = false;
        let (sx, sy) = (start % width, start / width);
        queue.push_back((to_u16(sx), to_u16(sy)));

        // The start pixel is always processed, so the bounds can start from it (no Option needed)
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (sx, sx, sy, sy);
        let mut area: usize = 0;
        let (mut sum_x, mut sum_y): (u64, u64) = (0, 0); // u64: no overflow even on huge blobs

        while let Some((qx, qy)) = queue.pop_front() {
            let x = usize::from(qx);
            let y = usize::from(qy);

            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
            area += 1;
            sum_x += u64::from(qx);
            sum_y += u64::from(qy);

            let xs = [x.checked_sub(1), Some(x), (x + 1 < width).then_some(x + 1)];
            let ys = [y.checked_sub(1), Some(y), (y + 1 < height).then_some(y + 1)];

            for ny in ys.into_iter().flatten() {
                for nx in xs.into_iter().flatten() {
                    let i = ny * width + nx;
                    if mask[i] {
                        mask[i] = false;
                        queue.push_back((to_u16(nx), to_u16(ny)));
                    }
                }
            }
        }

        // Filtering: small components are noise
        if area >= min_area {
            regions.push(Region {
                min_x,
                max_x,
                min_y,
                max_y,
                area,
                cx: mean(sum_x, area),
                cy: mean(sum_y, area),
            });
        }
    }
}

fn to_u16(v: usize) -> u16 {
    u16::try_from(v).expect("dimensions checked in main")
}

// Precision loss is negligible for a mean expressed in pixels
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn mean(sum: u64, n: usize) -> f32 {
    (sum as f64 / n as f64) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_centroid_and_filtering() {
        // 5x3 mask: a 2x2 block at the top left, an isolated pixel at (4, 2)
        #[rustfmt::skip]
        let mut mask = vec![
            true,  true,  false, false, false,
            true,  true,  false, false, false,
            false, false, false, false, true,
        ];
        let mut queue = VecDeque::new();
        let mut regions = Vec::new();

        find_regions(&mut mask, 5, 3, 1, &mut queue, &mut regions);
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].area, 4);
        assert!((regions[0].cx - 0.5).abs() < 1e-6 && (regions[0].cy - 0.5).abs() < 1e-6);
        assert_eq!(regions[1].area, 1);

        // Same mask with min_area = 2: the isolated pixel is filtered out
        #[rustfmt::skip]
        let mut mask = vec![
            true,  true,  false, false, false,
            true,  true,  false, false, false,
            false, false, false, false, true,
        ];
        find_regions(&mut mask, 5, 3, 2, &mut queue, &mut regions);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].area, 4);
    }
}
