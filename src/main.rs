use anyhow::Result;
use nokhwa::{   Camera, 
                utils::{CameraIndex,RequestedFormat,RequestedFormatType},
                pixel_format::RgbFormat
            };

fn main() -> Result<()> {
    let my_camera = Camera::new(
        CameraIndex::Index(0), 
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate)
        
    )?;
    Ok(())
}
