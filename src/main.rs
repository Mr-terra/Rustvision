mod pipeline;
use std::collections::VecDeque;

use anyhow::Result;
use minifb::{Key, KeyRepeat, Window, WindowOptions};
use nokhwa::{
    Camera,
    pixel_format::RgbFormat,
    utils::{CameraIndex, RequestedFormat, RequestedFormatType},
};
use pipeline::blobs::{Region, find_regions};

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
    let mut buffer: Vec<u32> = vec![0; size]; // display: minifb requires u32 (0RGB)
    let mut gray_buffer: Vec<u8> = vec![0; size]; // current frame in grayscale
    let mut background: Vec<u8> = vec![0; size]; // background image in grayscale
    let mut background_ready = false;

    // NEW: buffers for region detection, allocated once outside the hot loop
    let mut mask: Vec<bool> = vec![false; size]; // true = changed pixel not yet visited
    let mut queue: VecDeque<(u16, u16)> = VecDeque::with_capacity(size); // pixels to process
    let mut regions: Vec<Region> = Vec::with_capacity(64); // one box per component

    let mut threshold: u8 = 30;

    while !new_window.is_key_down(Key::Escape) && new_window.is_open() {
        let my_frame = my_camera.frame()?;
        let my_result = my_frame.decode_image::<RgbFormat>()?;

        // 1. RGB -> grayscale (single zip)
        for (gray, case) in my_result
            .pixels()
            .map(|p| from_u8_gray(p[0], p[1], p[2]))
            .zip(gray_buffer.iter_mut())
        {
            *case = gray;
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

        // find_regions(&mut mask, window_width, window_height, &mut queue, &mut regions);
        // println!("Regions: {}", regions.len()); // temporary, until boxes are drawn

        new_window.update_with_buffer(&buffer, window_width, window_height)?;

        if new_window.is_key_down(Key::Up) {
            threshold = threshold.saturating_add(1);
            println!("Threshold: {threshold}");
        } else if new_window.is_key_down(Key::Down) {
            threshold = threshold.saturating_sub(1);
            println!("Threshold: {threshold}");
        }
    }

    Ok(())
}

// grayscale as u8, rounded to nearest (max possible value is 255.0, so no overflow)
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
