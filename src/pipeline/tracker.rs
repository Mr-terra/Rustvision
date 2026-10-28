use super::blobs::Region;

/// Maximum distance (pixels) between a prediction and a detection to associate them.
pub const MAX_DIST: f32 = 50.0;
/// Upper bounds used to pre-allocate buffers (no allocation in the hot loop).
pub const MAX_TRACKS: usize = 32;
pub const MAX_DETECTIONS: usize = 64;

/// Consecutive matches needed to go from Recherche to Verrouille.
const LOCK_HITS: u32 = 3;
/// Missed frames before deleting a confirmed track (~1 s at 30 fps).
const MAX_LOST: u32 = 30;
/// Missed frames before deleting a track that was never confirmed (probably noise).
const MAX_LOST_TENTATIVE: u32 = 5;
/// Smoothing: weight of the measurement in the position (1.0 = raw centroid).
const ALPHA: f32 = 0.5;
/// Smoothing: weight of the new displacement in the velocity.
const BETA: f32 = 0.3;
/// Velocity damping while coasting without detection.
const DAMPING: f32 = 0.9;
/// Number of areas kept per track (circular buffer).
const AREA_HISTORY: usize = 4;
/// Frames spent in Recouvrement before falling back to the normal path.
const MAX_OVERLAP: u32 = 5;
/// Weight of the size term in the matching cost, in normal tracking.
const SIZE_WEIGHT: f32 = 0.5;
/// Weight of the size term for a track that has just left Recouvrement:
/// after an occlusion the position cannot tell a bounce from a crossing, the size can.
const SIZE_WEIGHT_AFTER_OVERLAP: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackState {
    Recherche,
    Verrouille { depuis: u32 },
    Perdu { images: u32 },
    /// Merged with the track `avec`: ignores detections, moves at constant velocity.
    Recouvrement { images: u32, avec: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Track {
    /// Unique id, never reused.
    pub id: u64,
    pub state: TrackState,
    /// Smoothed position.
    pub x: f32,
    pub y: f32,
    /// Half size of the last matched box.
    pub half_w: f32,
    pub half_h: f32,
    vx: f32,
    vy: f32,
    /// Consecutive matches while in Recherche.
    hits: u32,
    /// True once the track has reached Verrouille at least once.
    confirmed: bool,
    /// Last matched areas (circular buffer), frozen during Recouvrement.
    areas: [usize; AREA_HISTORY],
    /// Next slot to overwrite: always the oldest value.
    area_head: usize,
}

impl Track {
    fn new(id: u64, det: &Region) -> Self {
        Self {
            id,
            state: TrackState::Recherche,
            x: det.cx,
            y: det.cy,
            half_w: half_extent(det.min_x, det.max_x),
            half_h: half_extent(det.min_y, det.max_y),
            vx: 0.0,
            vy: 0.0,
            hits: 1,
            confirmed: false,
            // Filled with the first area so the mean is valid from the start
            areas: [det.area; AREA_HISTORY],
            area_head: 0,
        }
    }

    /// Expected position in the current frame (constant velocity).
    pub fn predicted(&self) -> (f32, f32) {
        (self.x + self.vx, self.y + self.vy)
    }

    /// Writes the new area over the oldest one, without shifting the array.
    fn push_area(&mut self, area: usize) {
        self.areas[self.area_head] = area;
        self.area_head = (self.area_head + 1) % AREA_HISTORY;
    }

    /// Mean of the history: reference size, less noisy than the last area alone.
    #[allow(clippy::cast_precision_loss)] // negligible for a size ratio
    fn mean_area(&self) -> f32 {
        let sum: usize = self.areas.iter().sum();
        sum as f32 / AREA_HISTORY as f32
    }

    /// True if the predicted boxes of the two tracks touch or overlap.
    fn touches(&self, other: &Track) -> bool {
        let (ax, ay) = self.predicted();
        let (bx, by) = other.predicted();
        (ax - bx).abs() <= self.half_w + other.half_w
            && (ay - by).abs() <= self.half_h + other.half_h
    }

    /// Matching cost, without unit: distance relative to the track size, plus the
    /// weighted relative change of size (sqrt of the area, a length like the distance).
    #[allow(clippy::cast_precision_loss)] // negligible for a size ratio
    fn cost(&self, det: &Region, d2: f32, size_weight: f32) -> f32 {
        let track_size = 2.0 * self.half_w.max(self.half_h);
        let rel_dist = d2.sqrt() / track_size;
        let referring_size = self.mean_area().sqrt();
        let real_size = ((det.area as f32).sqrt() - referring_size).abs() / referring_size;
        rel_dist + size_weight * real_size
    }

    /// Called when a detection is associated with this track.
    fn on_hit(&mut self, det: &Region) {
        // Weighted average between prediction and measurement (same idea as the background)
        let (px, py) = self.predicted();
        let nx = px + ALPHA * (det.cx - px);
        let ny = py + ALPHA * (det.cy - py);
        self.vx += BETA * ((nx - self.x) - self.vx);
        self.vy += BETA * ((ny - self.y) - self.vy);
        self.x = nx;
        self.y = ny;
        self.half_w = half_extent(det.min_x, det.max_x);
        self.half_h = half_extent(det.min_y, det.max_y);
        self.push_area(det.area);

        let state = self.state;
        self.state = match state {
            TrackState::Recherche => {
                self.hits += 1;
                if self.hits >= LOCK_HITS {
                    self.confirmed = true;
                    TrackState::Verrouille { depuis: 0 }
                } else {
                    TrackState::Recherche
                }
            }
            TrackState::Verrouille { depuis } => TrackState::Verrouille {
                depuis: depuis.saturating_add(1),
            },
            // Recouvrement never gets a pair; same exit as Perdu, as a safety net
            TrackState::Perdu { .. } | TrackState::Recouvrement { .. } => {
                if self.confirmed {
                    TrackState::Verrouille { depuis: 0 }
                } else {
                    self.hits = 1;
                    TrackState::Recherche
                }
            }
        };
    }

    /// Called when no detection is associated with this track.
    fn on_miss(&mut self) {
        // Keep moving along the prediction, slowing down to avoid drifting away
        self.x += self.vx;
        self.y += self.vy;
        self.vx *= DAMPING;
        self.vy *= DAMPING;

        let state = self.state;
        self.state = match state {
            // Recouvrement goes through on_overlap; Perdu rather than a panic if it ever gets here
            TrackState::Recherche
            | TrackState::Verrouille { .. }
            | TrackState::Recouvrement { .. } => TrackState::Perdu { images: 1 },
            TrackState::Perdu { images } => TrackState::Perdu {
                images: images.saturating_add(1),
            },
        };
    }

    /// Called for a track in Recouvrement: constant velocity, no damping,
    /// size and area history untouched (the merged region would corrupt them).
    fn on_overlap(&mut self) {
        self.x += self.vx;
        self.y += self.vy;
        if let TrackState::Recouvrement { images, avec } = self.state {
            self.state = TrackState::Recouvrement {
                images: images.saturating_add(1),
                avec,
            };
        }
    }

    /// State after a separation (same rule as leaving Perdu).
    fn resume(&mut self) {
        self.state = if self.confirmed {
            TrackState::Verrouille { depuis: 0 }
        } else {
            self.hits = 1;
            TrackState::Recherche
        };
    }

    fn is_dead(&self) -> bool {
        match self.state {
            // Recouvrement: the overlap counter decides, not the lost counter
            TrackState::Recherche
            | TrackState::Verrouille { .. }
            | TrackState::Recouvrement { .. } => false,
            TrackState::Perdu { images } => {
                let limit = if self.confirmed {
                    MAX_LOST
                } else {
                    MAX_LOST_TENTATIVE
                };
                images > limit
            }
        }
    }
}

#[allow(clippy::cast_precision_loss)] // sizes < 65536: exact in f32
fn half_extent(min: usize, max: usize) -> f32 {
    (max - min + 1) as f32 / 2.0
}

pub struct Tracker {
    tracks: Vec<Track>,
    next_id: u64,
    // Work buffers reused every frame
    pairs: Vec<(f32, usize, usize)>, // (cost, track index, detection index)
    track_used: Vec<bool>,
    det_used: Vec<bool>,
    nearest: Vec<Option<usize>>, // nearest detection of each track
    size_weight: Vec<f32>,       // weight of the size term for each track
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Tracker {
    pub fn new() -> Self {
        Self {
            tracks: Vec::with_capacity(MAX_TRACKS),
            next_id: 0,
            pairs: Vec::with_capacity(MAX_TRACKS * MAX_DETECTIONS),
            track_used: Vec::with_capacity(MAX_TRACKS),
            det_used: Vec::with_capacity(MAX_DETECTIONS),
            nearest: Vec::with_capacity(MAX_TRACKS),
            size_weight: Vec::with_capacity(MAX_TRACKS),
        }
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    /// Nearest detection to each prediction (plain distance, within `MAX_DIST`).
    fn find_nearest(&mut self, detections: &[Region]) {
        let max_d2 = MAX_DIST * MAX_DIST;
        self.nearest.clear();
        for track in &self.tracks {
            let (px, py) = track.predicted();
            let mut best: Option<(f32, usize)> = None;
            for (di, det) in detections.iter().enumerate() {
                let (dx, dy) = (det.cx - px, det.cy - py);
                let d2 = dx * dx + dy * dy;
                if d2 <= max_d2 && best.is_none_or(|(b, _)| d2 < b) {
                    best = Some((d2, di));
                }
            }
            self.nearest.push(best.map(|(_, di)| di));
        }
    }

    /// Enters, leaves or keeps the Recouvrement state, before any matching.
    /// Needs `self.nearest` filled for the current frame.
    #[allow(clippy::needless_range_loop)] // a track reads its partner while being modified
    fn update_overlaps(&mut self) {
        let n = self.tracks.len();
        self.size_weight.clear();
        self.size_weight.resize(n, SIZE_WEIGHT);

        // Tracks already merged: a separation wins over the time limit
        for ti in 0..n {
            let TrackState::Recouvrement { images, avec } = self.tracks[ti].state else {
                continue;
            };
            let separated = match self.tracks.iter().position(|t| t.id == avec) {
                None => true, // partner deleted: nothing left to wait for
                Some(pi) => {
                    self.nearest[ti].is_some()
                        && self.nearest[pi].is_some()
                        && self.nearest[ti] != self.nearest[pi]
                }
            };
            if separated {
                self.tracks[ti].resume();
                self.size_weight[ti] = SIZE_WEIGHT_AFTER_OVERLAP;
            } else if images >= MAX_OVERLAP {
                // Too long: back to the normal path, even if the track ends up lost
                self.tracks[ti].state = TrackState::Perdu { images };
            }
        }

        // Entry: two locked tracks whose predicted boxes touch and whose nearest
        // detection is the same (a single merged region for both)
        for i in 0..n {
            for j in i + 1..n {
                let (a, b) = (&self.tracks[i], &self.tracks[j]);
                let both_locked = matches!(a.state, TrackState::Verrouille { .. })
                    && matches!(b.state, TrackState::Verrouille { .. });
                if both_locked
                    && a.touches(b)
                    && self.nearest[i].is_some()
                    && self.nearest[i] == self.nearest[j]
                {
                    let ids = (a.id, b.id);
                    self.tracks[i].state = TrackState::Recouvrement { images: 0, avec: ids.1 };
                    self.tracks[j].state = TrackState::Recouvrement { images: 0, avec: ids.0 };
                }
            }
        }
    }

    pub fn update(&mut self, detections: &[Region]) {
        // 0. Recouvrement: decided before the matching
        self.find_nearest(detections);
        self.update_overlaps();

        // 1. Candidate pairs: cost without unit, gated by the max distance.
        //    Tracks in Recouvrement ignore detections: no pair for them.
        let max_d2 = MAX_DIST * MAX_DIST;
        self.pairs.clear();
        for (ti, track) in self.tracks.iter().enumerate() {
            if matches!(track.state, TrackState::Recouvrement { .. }) {
                continue;
            }
            let (px, py) = track.predicted();
            for (di, det) in detections.iter().enumerate() {
                let (dx, dy) = (det.cx - px, det.cy - py);
                let d2 = dx * dx + dy * dy;
                if d2 <= max_d2 {
                    let cost = track.cost(det, d2, self.size_weight[ti]);
                    self.pairs.push((cost, ti, di));
                }
            }
        }

        // 2. Sort by increasing cost. sort_unstable does not allocate (sort_by does)
        self.pairs.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));

        // 3. Global greedy: accept a pair only if both sides are still free
        self.track_used.clear();
        self.track_used.resize(self.tracks.len(), false);
        self.det_used.clear();
        self.det_used.resize(detections.len(), false);
        for &(_, ti, di) in &self.pairs {
            if self.track_used[ti] || self.det_used[di] {
                continue;
            }
            self.track_used[ti] = true;
            self.det_used[di] = true;
            self.tracks[ti].on_hit(&detections[di]);
        }

        // 3b. The merged region of a track in Recouvrement is marked as taken,
        //     otherwise step 6 would create a new track for it
        for (ti, track) in self.tracks.iter().enumerate() {
            if let (TrackState::Recouvrement { .. }, Some(di)) = (track.state, self.nearest[ti]) {
                self.det_used[di] = true;
            }
        }

        // 4. Track without detection: not deleted right away
        for (track, &used) in self.tracks.iter_mut().zip(&self.track_used) {
            if used {
                continue;
            }
            match track.state {
                TrackState::Recouvrement { .. } => track.on_overlap(),
                TrackState::Recherche | TrackState::Verrouille { .. } | TrackState::Perdu { .. } => {
                    track.on_miss();
                }
            }
        }

        // 5. Delete tracks lost for too long (retain does not allocate)
        self.tracks.retain(|t| !t.is_dead());

        // 6. Detection without track: new track, with an id that never goes back
        for (det, &used) in detections.iter().zip(&self.det_used) {
            if !used && self.tracks.len() < MAX_TRACKS {
                self.tracks.push(Track::new(self.next_id, det));
                self.next_id += 1; // u64: cannot overflow in practice
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 10x10 detection centred on (x, y)
    fn det(x: u16, y: u16) -> Region {
        let (ux, uy) = (usize::from(x), usize::from(y));
        Region {
            min_x: ux - 5,
            max_x: ux + 4,
            min_y: uy - 5,
            max_y: uy + 4,
            area: 100,
            cx: f32::from(x),
            cy: f32::from(y),
        }
    }

    /// w x w detection centred on (x, y)
    fn det_w(x: u16, y: u16, w: u16) -> Region {
        let (ux, uy, uw) = (usize::from(x), usize::from(y), usize::from(w));
        Region {
            min_x: ux - uw / 2,
            max_x: ux - uw / 2 + uw - 1,
            min_y: uy - uw / 2,
            max_y: uy - uw / 2 + uw - 1,
            area: uw * uw,
            cx: f32::from(x),
            cy: f32::from(y),
        }
    }

    /// One frame with two square objects on the line y = 100:
    /// a single merged region when their boxes overlap, two regions otherwise.
    #[allow(clippy::cast_precision_loss)]
    fn scene(xa: u16, wa: u16, xb: u16, wb: u16) -> Vec<Region> {
        let (a, b) = (det_w(xa, 100, wa), det_w(xb, 100, wb));
        if u32::from(xa.abs_diff(xb)) * 2 < u32::from(wa) + u32::from(wb) {
            let area = a.area + b.area;
            let cx = (a.cx * a.area as f32 + b.cx * b.area as f32) / area as f32;
            vec![Region {
                min_x: a.min_x.min(b.min_x),
                max_x: a.max_x.max(b.max_x),
                min_y: a.min_y.min(b.min_y),
                max_y: a.max_y.max(b.max_y),
                area,
                cx,
                cy: 100.0,
            }]
        } else {
            vec![a, b]
        }
    }

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    /// Id of the track closest to position x on the line (u64::MAX if no track)
    fn closest_id(tracks: &[Track], x: f32) -> u64 {
        tracks
            .iter()
            .min_by(|a, b| (a.x - x).abs().total_cmp(&(b.x - x).abs()))
            .map_or(u64::MAX, |t| t.id)
    }

    #[test]
    fn greedy_example_two_tracks_two_detections() {
        let mut t = Tracker::new();
        t.update(&[det(100, 100), det(160, 100)]); // A = id 0, B = id 1

        t.update(&[det(135, 100), det(175, 100)]);
        let tr = t.tracks();
        assert_eq!(tr.len(), 2, "no new track must be created");
        assert_eq!((tr[0].id, tr[1].id), (0, 1));
        assert!(approx(tr[0].x, 117.5)); // A took detection 1
        assert!(approx(tr[1].x, 167.5)); // B took detection 2
    }

    #[test]
    fn detection_order_does_not_matter() {
        let mut t = Tracker::new();
        t.update(&[det(100, 100), det(160, 100)]);
        t.update(&[det(175, 100), det(135, 100)]); // reversed order
        let tr = t.tracks();
        assert_eq!(tr.len(), 2);
        assert!(approx(tr[0].x, 117.5));
        assert!(approx(tr[1].x, 167.5));
    }

    #[test]
    fn far_detection_creates_new_track() {
        let mut t = Tracker::new();
        t.update(&[det(100, 100)]);
        t.update(&[det(300, 100)]);
        let tr = t.tracks();
        assert_eq!(tr.len(), 2);
        assert_eq!(tr[0].state, TrackState::Perdu { images: 1 });
        assert_eq!(tr[1].id, 1);
        assert_eq!(tr[1].state, TrackState::Recherche);
    }

    #[test]
    fn locks_after_lock_hits_frames() {
        let mut t = Tracker::new();
        for _ in 0..LOCK_HITS - 1 {
            t.update(&[det(100, 100)]);
            assert_eq!(t.tracks()[0].state, TrackState::Recherche);
        }
        t.update(&[det(100, 100)]);
        assert_eq!(t.tracks()[0].state, TrackState::Verrouille { depuis: 0 });
        t.update(&[det(100, 100)]);
        assert_eq!(t.tracks()[0].state, TrackState::Verrouille { depuis: 1 });
    }

    #[test]
    fn lost_track_is_deleted_and_id_not_reused() {
        let mut t = Tracker::new();
        for _ in 0..LOCK_HITS {
            t.update(&[det(100, 100)]);
        }
        for _ in 0..MAX_LOST {
            t.update(&[]);
        }
        assert_eq!(t.tracks()[0].state, TrackState::Perdu { images: MAX_LOST });

        t.update(&[]);
        assert!(t.tracks().is_empty());

        t.update(&[det(100, 100)]);
        assert_eq!(t.tracks()[0].id, 1);
    }

    #[test]
    fn lost_confirmed_track_relocks_when_found() {
        let mut t = Tracker::new();
        for _ in 0..LOCK_HITS {
            t.update(&[det(100, 100)]);
        }
        t.update(&[]);
        t.update(&[det(100, 100)]);
        assert_eq!(t.tracks()[0].state, TrackState::Verrouille { depuis: 0 });
    }

    #[test]
    fn crossing_keeps_identities() {
        // Merged from step 33 to 37, separation at step 38
        let mut t = Tracker::new();
        for step in 0u16..=45 {
            t.update(&scene(100 + 2 * step, 10, 240 - 2 * step, 10));
            if step == 35 {
                assert_eq!(t.tracks().len(), 2, "the merged region must not create a track");
                assert!(t
                    .tracks()
                    .iter()
                    .all(|tr| matches!(tr.state, TrackState::Recouvrement { .. })));
            }
        }
        let tr = t.tracks();
        assert_eq!(tr.len(), 2);
        // At step 45, A is at 190 and B at 150: they have crossed
        assert_eq!(closest_id(tr, 190.0), 0, "A must keep its id");
        assert_eq!(closest_id(tr, 150.0), 1, "B must keep its id");
    }

    #[test]
    fn bounce_with_different_sizes_keeps_identities() {
        // Contact at step 30, then both go back. Merged from 28 to 32:
        // at step 33 the separation and the time limit happen together
        let mut t = Tracker::new();
        for step in 0u16..=36 {
            let (xa, xb) = if step <= 30 {
                (100 + 3 * step, 280 - 3 * step)
            } else {
                (190 - 3 * (step - 30), 190 + 3 * (step - 30))
            };
            t.update(&scene(xa, 10, xb, 16));
        }
        let tr = t.tracks();
        assert_eq!(tr.len(), 2, "no new track must be created");
        // At step 36, A is back at 172 and B at 208
        assert_eq!(closest_id(tr, 172.0), 0, "the size must keep A's id on A");
        assert_eq!(closest_id(tr, 208.0), 1, "the size must keep B's id on B");
    }

    #[test]
    fn bounce_with_same_size_swaps_identities() {
        // Known limit: with identical objects, nothing tells a bounce from a crossing
        let mut t = Tracker::new();
        for step in 0u16..=36 {
            let (xa, xb) = if step <= 30 {
                (100 + 3 * step, 280 - 3 * step)
            } else {
                (190 - 3 * (step - 30), 190 + 3 * (step - 30))
            };
            t.update(&scene(xa, 10, xb, 10));
        }
        let tr = t.tracks();
        assert_eq!(tr.len(), 2, "no new track must be created");
        // At step 36, A is back at 172 and B at 208: the identities have swapped
        assert_eq!(closest_id(tr, 208.0), 0, "id 0 must follow B after the swap");
        assert_eq!(closest_id(tr, 172.0), 1, "id 1 must follow A after the swap");
    }

    #[test]
    fn stopped_object_never_takes_the_other_id() {
        // Expected: A keeps id 0; B keeps id 1 or gets a new id, but never id 0
        let mut t = Tracker::new();
        for step in 0u16..=45 {
            let xb = if step <= 25 { 280 - 3 * step } else { 205 }; // B stops at 205
            t.update(&scene(100 + 3 * step, 10, xb, 10));
        }
        let tr = t.tracks();
        // At step 45, A is at 235 and B is still at 205
        assert_eq!(closest_id(tr, 235.0), 0, "A must keep its id");
        assert_ne!(closest_id(tr, 205.0), 0, "B must never take A's id");
    }

    #[test]
    fn long_overlap_falls_back_to_normal_handling() {
        // Contact at step 29, then the two objects stay stuck together
        let mut t = Tracker::new();
        for step in 0u16..=40 {
            let s = step.min(29);
            t.update(&scene(100 + 3 * s, 10, 280 - 3 * s, 10));
            if step == 33 {
                assert!(t
                    .tracks()
                    .iter()
                    .all(|tr| matches!(tr.state, TrackState::Recouvrement { .. })));
            }
        }
        let tr = t.tracks();
        assert_eq!(tr.len(), 2, "the merged region must not create a track");
        // After MAX_OVERLAP frames, one track takes the region again, the other is lost
        let locked = tr
            .iter()
            .filter(|x| matches!(x.state, TrackState::Verrouille { .. }))
            .count();
        let lost = tr
            .iter()
            .filter(|x| matches!(x.state, TrackState::Perdu { .. }))
            .count();
        assert_eq!((locked, lost), (1, 1));
    }
}