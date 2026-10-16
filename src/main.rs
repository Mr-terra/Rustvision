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
    let mut my_frame = my_camera.frame()?;

    let window_width = my_camera.camera_format().width().try_into()?;
    let window_height = my_camera.camera_format().height().try_into()?;

    let mut new_window = Window::new(
        "Test",
        window_width,
        window_height,
        WindowOptions::default(),
    )?;

    let mut buffer: Vec<u32> = vec![0; window_width * window_height];


    while !new_window.is_key_down(Key::Escape) && new_window.is_open() {

        my_frame = my_camera.frame()?;
        let my_result = my_frame.decode_image::<RgbFormat>()?;

        for (colour, case) in my_result
            .pixels()
            .map(|p| from_u8_rgb(p[0], p[1], p[2]))
            .zip(buffer.iter_mut())
        {
            *case = colour;
        }

        new_window.update_with_buffer(&buffer, window_width, window_height)?;
    }

    Ok(())
}

fn from_u8_rgb(r: u8, g: u8, b: u8) -> u32 {
    let (mut r,mut g, mut b) = (r as f32, g as f32, b as f32);
    r =   0.299 * r;
    g =   0.587 * g;
    b =   0.114 * b;
    let gray = r + g + b;
    ((gray as u32) << 16) | ((gray as u32) << 8) | (gray as u32)
}
