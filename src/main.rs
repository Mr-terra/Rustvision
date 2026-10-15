use anyhow::Result;
use nokhwa::{Camera,utils::{CameraIndex,RequestedFormat,RequestedFormatType},pixel_format::RgbFormat};

fn main() -> Result<()> {
    
    let mut my_camera = Camera::new(
        CameraIndex::Index(0), 
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate)
        
    )?;
    my_camera.open_stream()?;
    let my_frame = my_camera.frame()?;

    let my_result = my_frame.decode_image::<RgbFormat>()?;

    println!("Frame size: {}x{}", my_result.width(), my_result.height());

    Ok(())
}
