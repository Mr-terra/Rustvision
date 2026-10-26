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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackState {
    Recherche,
    Verrouille { depuis: u32 },
    Perdu { images: u32 },
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
        }
    }

    /// Expected position in the current frame (constant velocity).
    pub fn predicted(&self) -> (f32, f32) {
        (self.x + self.vx, self.y + self.vy)
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
            TrackState::Perdu { .. } => {
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
            TrackState::Recherche | TrackState::Verrouille { .. } => {
                TrackState::Perdu { images: 1 }
            }
            TrackState::Perdu { images } => TrackState::Perdu {
                images: images.saturating_add(1),
            },
        };
    }

    fn is_dead(&self) -> bool {
        match self.state {
            TrackState::Recherche | TrackState::Verrouille { .. } => false,
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
    pairs: Vec<(f32, usize, usize)>, // (squared distance, track index, detection index)
    track_used: Vec<bool>,
    det_used: Vec<bool>,
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
        }
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn update(&mut self, detections: &[Region]) {
        // 1. Candidate pairs: squared distance prediction -> centroid, below the max distance
        let max_d2 = MAX_DIST * MAX_DIST;
        self.pairs.clear();
        for (ti, track) in self.tracks.iter().enumerate() {
            let (px, py) = track.predicted();
            for (di, det) in detections.iter().enumerate() {
                let (dx, dy) = (det.cx - px, det.cy - py);
                let d2 = dx * dx + dy * dy;
                if d2 <= max_d2 {
                    self.pairs.push((d2, ti, di));
                }
            }
        }

        // 2. Sort by increasing distance. sort_unstable does not allocate (sort_by does)
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

        // 4. Track without detection: not deleted right away
        for (track, &used) in self.tracks.iter_mut().zip(&self.track_used) {
            if !used {
                track.on_miss();
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

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
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
}
