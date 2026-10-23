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
    queue: &mut VecDeque<(usize, usize)>,
    regions: &mut Vec<Region>,
) {
    regions.clear();
    // Detection: for each pixel still true -> set to false, push to queue, bounds to None
    // Processing: while the queue is not empty -> update bounds, check the 8 neighbors
    // End: if let on the 4 bounds -> regions.push(...)
}