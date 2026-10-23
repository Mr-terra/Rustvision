use std::collections::VecDeque;

/// Bounding box of a connected component.
#[derive(Debug, PartialEq)]
pub struct Region {
    pub min_x: usize,
    pub max_x: usize,
    pub min_y: usize,
    pub max_y: usize,
}

/// Finds the 8-connected components of the mask and stores their boxes in `regions`.
/// The mask is consumed: every pixel ends up `false`.
pub fn find_regions(
    mask: &mut [bool],
    width: usize,
    height: usize,
    queue: &mut VecDeque<(u16, u16)>,
    regions: &mut Vec<Region>,
) {
    // Contract: the caller guarantees width and height fit in u16 (checked in main),
    // so the `as u16` casts below cannot truncate.
    debug_assert!(u16::try_from(width).is_ok() && u16::try_from(height).is_ok());
    debug_assert_eq!(mask.len(), width * height);

    regions.clear();

    for start in 0..mask.len() {
        // Detection: a pixel still true starts a new component
        if !mask[start] {
            continue;
        }
        mask[start] = false; // marked when it enters the queue
        queue.push_back(((start % width) as u16, (start / width) as u16));
        let (mut min_x, mut max_x, mut min_y, mut max_y): (
            Option<usize>,
            Option<usize>,
            Option<usize>,
            Option<usize>,
        ) = (None, None, None, None);

        // Processing: while the queue is not empty
        while let Some((qx, qy)) = queue.pop_front() {
            let x = usize::from(qx);
            let y = usize::from(qy);

            min_x = Some(min_x.map_or(x, |v| v.min(x)));
            max_x = Some(max_x.map_or(x, |v| v.max(x)));
            min_y = Some(min_y.map_or(y, |v| v.min(y)));
            max_y = Some(max_y.map_or(y, |v| v.max(y)));

            // Candidate coordinates; None = outside the image, skipped by flatten()
            let xs = [x.checked_sub(1), Some(x), (x + 1 < width).then_some(x + 1)];
            let ys = [y.checked_sub(1), Some(y), (y + 1 < height).then_some(y + 1)];

            for ny in ys.into_iter().flatten() {
                for nx in xs.into_iter().flatten() {
                    let i = ny * width + nx;
                    // (x, y) itself is already false, so it is skipped here
                    if mask[i] {
                        mask[i] = false;
                        queue.push_back((nx as u16, ny as u16));
                    }
                }
            }
        }

        // End: queue empty -> store the box
        if let (Some(min_x), Some(max_x), Some(min_y), Some(max_y)) = (min_x, max_x, min_y, max_y) {
            regions.push(Region {
                min_x,
                max_x,
                min_y,
                max_y,
            });
        }
    }
}
