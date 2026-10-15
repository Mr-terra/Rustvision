use anyhow::Result;
use nokhwa::{Camera,utils::{CameraIndex,RequestedFormat,RequestedFormatType},pixel_format::RgbFormat};
use minifb::{Window, WindowOptions, Key};

fn main() -> Result<()> {
    
    // setup camera
    let mut my_camera = Camera::new(CameraIndex::Index(0),RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate))?;
    my_camera.open_stream()?;
    // setup window
    let mut my_frame = my_camera.frame()?;
    let mut my_result = my_frame.decode_image::<RgbFormat>()?;

    let  window_width = my_result.width().try_into()?;
    let  window_height = my_result.height().try_into()?;

    let mut new_window = Window::new("Test", window_width, window_height, WindowOptions::default())?;
    let mut buffer: Vec<u32> = my_result.pixels().map(|pixel| {from_u8_rgb(pixel[0], pixel[1], pixel[2])}).collect::<Vec<u32>>();

    
    
    
    while !new_window.is_key_down(Key::Escape) && new_window.is_open() {

        my_frame = my_camera.frame()?;
        my_result = my_frame.decode_image::<RgbFormat>()?;
        
        for (colour, case) in my_result.pixels().map(|p| from_u8_rgb(p[0], p[1], p[2])).zip(buffer.iter_mut()) {
            *case = colour;
        }

        new_window.update_with_buffer(&buffer, window_width, window_height)?;
        
    }

    Ok(())
}

fn from_u8_rgb(r: u8, g: u8, b: u8) -> u32 {
    let (r, g, b) = (r as u32, g as u32, b as u32);
    (r << 16) | (g << 8) | b
}