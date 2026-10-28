mod pipeline;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use anyhow::Result;
use minifb::{Key, KeyRepeat, Window, WindowOptions};
use nokhwa::{
    Camera,
    pixel_format::RgbFormat,
    utils::{CameraIndex, RequestedFormat, RequestedFormatType},
};
use pipeline::blobs::{Region, find_regions};
use pipeline::display::draw_tracks;
use pipeline::tracker::{MAX_DETECTIONS, Tracker};

/// Without a valid frame for this long, the camera is considered unplugged.
const CAMERA_TIMEOUT: Duration = Duration::from_secs(1);

/// What the loop does after a failed capture.
#[derive(Debug, PartialEq, Eq)]
enum CaptureAction {
    /// Isolated failure: skip this frame and try again.
    SkipFrame,
    /// No valid frame for too long: the camera is considered unplugged.
    Exit,
}

/// Pure decision: depends only on the time since the last valid frame,
/// so it can be tested without a camera and without waiting.
fn on_capture_error(since_last_ok: Duration) -> CaptureAction {
    if since_last_ok > CAMERA_TIMEOUT {
        CaptureAction::Exit
    } else {
        CaptureAction::SkipFrame
    }
}

fn main() -> Result<()> {
    // setup camera
    let mut my_camera = Camera::new(
        CameraIndex::Index(0),
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate),
    )?;
    my_camera.open_stream()?;

    // setup window
    let window_width = my_camera.camera_format().width().try_into()?;
    let window_height = my_camera.camera_format().height().try_into()?;

    // Queue coordinates are stored as u16: reject resolutions that don't fit
    u16::try_from(window_width)?;
    u16::try_from(window_height)?;

    let mut new_window = Window::new(
        "Test",
        window_width,
        window_height,
        WindowOptions::default(),
    )?;

    let size = window_width * window_height;
    // RGB frame decoded in place: 3 bytes per pixel, so always a multiple of 3
    let mut rgb_buffer: Vec<u8> = vec![0; size * 3];
    let mut buffer: Vec<u32> = vec![0; size]; // display: minifb requires u32 (0RGB)
    let mut gray_buffer: Vec<u8> = vec![0; size]; // current frame in grayscale
    let mut background: Vec<u8> = vec![0; size]; // background image in grayscale
    let mut background_ready = false;

    // buffers for region detection, allocated once outside the hot loop
    let mut mask: Vec<bool> = vec![false; size]; // true = changed pixel not yet visited
    let mut queue: VecDeque<(u16, u16)> = VecDeque::with_capacity(size); // pixels to process
    let mut regions: Vec<Region> = Vec::with_capacity(MAX_DETECTIONS); // one box per component

    // tracking: tracks and work buffers are pre-allocated inside the Tracker
    let mut tracker = Tracker::new();

    let mut threshold: u8 = 90;
    let mut min_area: usize = 50; // components below this are noise

    // Time of the last valid frame; starts when the stream opens,
    // so a failure on the very first frame is handled the same way
    let mut last_frame_ok = Instant::now();

    while !new_window.is_key_down(Key::Escape) && new_window.is_open() {
        // 0. Capture and decode into the reused buffer
        let captured = my_camera
            .frame()
            .and_then(|frame| frame.decode_image_to_buffer::<RgbFormat>(&mut rgb_buffer));
        if let Err(err) = captured {
            match on_capture_error(last_frame_ok.elapsed()) {
                CaptureAction::Exit => {
                    anyhow::bail!("camera unavailable for more than {CAMERA_TIMEOUT:?}: {err}");
                }
                CaptureAction::SkipFrame => {
                    new_window.update(); // keep the window responsive
                    continue;
                }
            }
        }
        last_frame_ok = Instant::now();

        // 1. RGB -> grayscale, 3 bytes at a time
        for (rgb, gray) in rgb_buffer.chunks_exact(3).zip(gray_buffer.iter_mut()) {
            *gray = from_u8_gray(rgb[0], rgb[1], rgb[2]);
        }

        // 2. Capture the background: first frame, then each time Space is pressed
        if !background_ready || new_window.is_key_pressed(Key::Space, KeyRepeat::No) {
            background.copy_from_slice(&gray_buffer);
            background_ready = true;
            println!("Background captured");
        }

        // 3. Triple zip: current frame + background -> display buffer + mask
        for (((&gray, &bg), case), is_changed) in gray_buffer
            .iter()
            .zip(background.iter())
            .zip(buffer.iter_mut())
            .zip(mask.iter_mut())
        {
            *case = diff_threshold(gray.abs_diff(bg), threshold);
            *is_changed = *case != 0; // white pixel = changed
        }

        // 4. Group changed pixels into regions, small ones are filtered out as noise
        find_regions(
            &mut mask,
            window_width,
            window_height,
            min_area,
            &mut queue,
            &mut regions,
        );

        // 5. Associate detections with tracks and update their states
        tracker.update(&regions);

        // 6. Draw each track (box, centre, id, color by state), in place, before display
        draw_tracks(&mut buffer, window_width, window_height, tracker.tracks());

        new_window.update_with_buffer(&buffer, window_width, window_height)?;

        // Settings move one step per frame while held; printed once, on release
        if new_window.is_key_down(Key::Up) {
            threshold = threshold.saturating_add(1);
        } else if new_window.is_key_down(Key::Down) {
            threshold = threshold.saturating_sub(1);
        }
        if new_window.is_key_released(Key::Up) || new_window.is_key_released(Key::Down) {
            println!("Threshold: {threshold}");
        }

        if new_window.is_key_down(Key::Right) {
            min_area = min_area.saturating_add(5);
        } else if new_window.is_key_down(Key::Left) {
            min_area = min_area.saturating_sub(5);
        }
        if new_window.is_key_released(Key::Right) || new_window.is_key_released(Key::Left) {
            println!("Min area: {min_area}");
        }
    }

    Ok(())
}

// grayscale as u8, rounded to nearest (max is ~255.0 in f32, round() keeps it at 255, so no overflow)
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn from_u8_gray(r: u8, g: u8, b: u8) -> u8 {
    let gray = 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b);
    gray.round() as u8
}

// white if the pixel changed enough compared to the background, black otherwise
fn diff_threshold(diff: u8, threshold: u8) -> u32 {
    // >= makes the threshold inclusive
    if diff >= threshold {
        0x00FF_FFFF // white
    } else {
        0 // black
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_failure_skips_the_frame() {
        assert_eq!(on_capture_error(Duration::ZERO), CaptureAction::SkipFrame);
        assert_eq!(on_capture_error(Duration::from_millis(500)), CaptureAction::SkipFrame);
    }

    #[test]
    fn exactly_the_timeout_still_skips() {
        // The limit is strict (>): one second exactly is still tolerated
        assert_eq!(on_capture_error(CAMERA_TIMEOUT), CaptureAction::SkipFrame);
    }

    #[test]
    fn long_failure_exits() {
        assert_eq!(on_capture_error(Duration::from_millis(1500)), CaptureAction::Exit);
    }
}