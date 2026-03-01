use anyhow::Result;
use image::{DynamicImage, imageops::FilterType};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

const UPPER_HALF: char = '\u{2580}'; // ▀

/// Convert a DynamicImage to ratatui Lines using half-block characters.
/// Each terminal row represents 2 pixel rows (upper=fg, lower=bg on ▀).
pub fn image_to_halfblocks(img: &DynamicImage, width: u16, height: u16) -> Vec<Line<'static>> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let resized = img.resize_exact(width as u32, (height * 2) as u32, FilterType::Triangle);
    let rgb = resized.to_rgb8();
    let mut lines = Vec::with_capacity(height as usize);

    for row in 0..height {
        let mut spans = Vec::with_capacity(width as usize);
        let upper_y = (row * 2) as u32;
        let lower_y = upper_y + 1;

        for col in 0..width {
            let up = rgb.get_pixel(col as u32, upper_y);
            let lo = rgb.get_pixel(col as u32, lower_y);

            let fg = Color::Rgb(up[0], up[1], up[2]);
            let bg = Color::Rgb(lo[0], lo[1], lo[2]);

            spans.push(Span::styled(
                UPPER_HALF.to_string(),
                Style::default().fg(fg).bg(bg),
            ));
        }

        lines.push(Line::from(spans));
    }

    lines
}

/// Download and decode an image from a URL (async).
pub async fn download_avatar(url: &str) -> Result<DynamicImage> {
    let bytes = reqwest::get(url).await?.bytes().await?;
    let img = image::load_from_memory(&bytes)?;
    Ok(img)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{RgbImage, DynamicImage};

    #[test]
    fn test_halfblocks_basic() {
        let mut img = RgbImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgb([255, 0, 0]));
        img.put_pixel(1, 0, image::Rgb([0, 255, 0]));
        img.put_pixel(0, 1, image::Rgb([0, 0, 255]));
        img.put_pixel(1, 1, image::Rgb([255, 255, 0]));

        let dyn_img = DynamicImage::ImageRgb8(img);
        let lines = image_to_halfblocks(&dyn_img, 2, 1);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 2);
    }

    #[test]
    fn test_halfblocks_zero_size() {
        let img = DynamicImage::ImageRgb8(RgbImage::new(1, 1));
        assert!(image_to_halfblocks(&img, 0, 0).is_empty());
        assert!(image_to_halfblocks(&img, 0, 5).is_empty());
        assert!(image_to_halfblocks(&img, 5, 0).is_empty());
    }
}
