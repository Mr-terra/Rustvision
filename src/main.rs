use anyhow::Result;
use minifb::{Key, Window, WindowOptions};
use nokhwa::{
    Camera,
    pixel_format::RgbFormat,
    utils::{CameraIndex, RequestedFormat, RequestedFormatType},
};

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

    let mut new_window = Window::new(
        "Test",
        window_width,
        window_height,
        WindowOptions::default(),
    )?;

    let mut buffer: Vec<u32> = vec![0; window_width * window_height];

    let mut threshold = 127.0;

    while !new_window.is_key_down(Key::Escape) && new_window.is_open() {
        let my_frame = my_camera.frame()?;
        let my_result = my_frame.decode_image::<RgbFormat>()?;

        for (colour, case) in my_result
            .pixels()
            .map(|p| from_u8_gray(p[0], p[1], p[2], threshold))
            .zip(buffer.iter_mut())
        {
            *case = colour;
        }

        new_window.update_with_buffer(&buffer, window_width, window_height)?;

        if new_window.is_key_down(Key::Up) {
            threshold += 1.0;
            if threshold > 255.0 {
                threshold = 255.0;
            }
            println!("Threshold : {threshold}");
        } else if new_window.is_key_down(Key::Down) {
            threshold -= 1.0;
            if threshold < 0.0 {
                threshold = 0.0;
            }
            println!("Threshold : {threshold}");
        }
    }

    Ok(())
}

//old function to get colour
#[allow(dead_code)]
#[allow(clippy::cast_lossless)]
fn from_u8_rgb(r: u8, g: u8, b: u8) -> u32 {
    let (r, g, b) = (r as u32, g as u32, b as u32);
    (r << 16) | (g << 8) | b
}

// function to get gray from rgb and threshold
#[allow(clippy::cast_lossless)]
fn from_u8_gray(r: u8, g: u8, b: u8, threshold: f32) -> u32 {
    let (mut r, mut g, mut b) = (r as f32, g as f32, b as f32);
    r *= 0.299;
    g *= 0.587;
    b *= 0.114;
    let gray = r + g + b;
    gray_threshold(gray, threshold)
}

fn gray_threshold(gray: f32, threshold: f32) -> u32 {
    // we are using > or = to make sure that the threshold is inclusive
    if gray >= threshold {
        0x00FF_FFFF // white
    } else {
        0 // black
    }
}
