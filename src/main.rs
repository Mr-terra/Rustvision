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
            .map(|p| from_u8_rgb(p[0], p[1], p[2], threshold))
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
            println!("Threshold : {}", threshold);
        } else if new_window.is_key_down(Key::Down) {
            threshold -= 1.0;
            if threshold < 0.0 {
                threshold = 0.0;
            }
            println!("Threshold : {}", );
        }
    }

    Ok(())
}

fn from_u8_rgb(r: u8, g: u8, b: u8, threshold: f32) -> u32 {
    let (mut r, mut g, mut b) = (r as f32, g as f32, b as f32);
    r = 0.299 * r;
    g = 0.587 * g;
    b = 0.114 * b;
    let mut gray = r + g + b;
    gray = gray_threshold(gray, threshold);
    gray as u32
}

fn gray_threshold(mut gray: f32, threshold: f32) -> f32 {
    // we are using > or = to make sure that the threshold is inclusive
    if gray >= threshold {
        gray = 255.0;
    } else {
        gray = 0.0;
    }
    gray
}
