//! Cache independently positioned raster bands so clipped previews cannot enter the composer.

use std::io;
use std::io::Cursor;
use std::io::Write;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;

use base64::Engine;
use crossterm::cursor::MoveTo;
use crossterm::queue;
use image::imageops::FilterType;

use crate::terminal_images::ImageProtocol;

use super::ImagePlacement;

static CELL_SIZE: AtomicU32 = AtomicU32::new(/*v*/ 0);

/// Recognize only complete CSI cell-size reports, never modified navigation keys.
pub(crate) fn cell_size_report(input: &[u8]) -> Option<(usize, u16, u16)> {
    let payload = input.strip_prefix(b"\x1b[6;")?;
    let end = payload
        .iter()
        .take(/*n*/ 12)
        .position(|byte| *byte == b't')?;
    let payload = std::str::from_utf8(&payload[..end]).ok()?;
    let (height, width) = payload.split_once(';')?;
    if !height.bytes().all(|byte| byte.is_ascii_digit())
        || !width.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let height = height.parse::<u16>().ok()?;
    let width = width.parse::<u16>().ok()?;
    (height > 0 && width > 0).then_some((end + 5, width, height))
}

#[cfg(unix)]
pub(crate) fn observe_cell_size(input: &[u8]) {
    for start in 0..input.len() {
        if let Some((_, width, height)) = cell_size_report(&input[start..]) {
            CELL_SIZE.store(
                (u32::from(width) << 16) | u32::from(height),
                Ordering::Relaxed,
            );
        }
    }
}

pub(super) fn cell_size() -> (u16, u16) {
    let size = crossterm::terminal::window_size()
        .ok()
        .filter(|size| size.columns > 0 && size.rows > 0 && size.width > 0 && size.height > 0);
    let queried = CELL_SIZE.load(Ordering::Relaxed);
    let (width, height) = size
        .map(|size| (size.width / size.columns, size.height / size.rows))
        .unwrap_or_else(|| {
            if queried != 0 {
                ((queried >> 16) as u16, queried as u16)
            } else {
                (4, 8)
            }
        });
    (
        width.clamp(/*min*/ 1, /*max*/ 16),
        height.clamp(/*min*/ 1, /*max*/ 32),
    )
}

pub(super) struct RasterRows {
    pub(super) geometry: (u16, u16, (u16, u16)),
    bands: Vec<Vec<u8>>,
}

impl RasterRows {
    pub(super) fn new(
        raster: &image::RgbaImage,
        geometry: (u16, u16, (u16, u16)),
        protocol: ImageProtocol,
    ) -> Option<Self> {
        let (columns, rows, (cell_width, cell_height)) = geometry;
        let width = u32::from(columns) * u32::from(cell_width);
        let height = u32::from(rows) * u32::from(cell_height);
        let raster = image::imageops::resize(raster, width, height, FilterType::Triangle);
        let bands = (0..rows)
            .map(|row| {
                let band = image::imageops::crop_imm(
                    &raster,
                    /*x*/ 0,
                    u32::from(row) * u32::from(cell_height),
                    width,
                    u32::from(cell_height),
                )
                .to_image();
                if protocol == ImageProtocol::Iterm {
                    let mut png = Cursor::new(Vec::new());
                    image::DynamicImage::ImageRgba8(band).write_to(&mut png, image::ImageFormat::Png).ok()?;
                    let png = png.into_inner();
                    let payload = base64::engine::general_purpose::STANDARD.encode(&png);
                    Some(format!("\x1b]1337;File=inline=1;size={};width={columns};height=1;preserveAspectRatio=0:{payload}\x07", png.len()).into_bytes())
                } else {
                    crate::pets::sixel::encode_rgba(band.as_raw(), width, u32::from(cell_height)).ok()
                }
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self { geometry, bands })
    }

    pub(super) fn draw(
        &self,
        writer: &mut impl Write,
        placement: &ImagePlacement,
    ) -> io::Result<()> {
        for row in 0..placement.area.height {
            queue!(writer, MoveTo(placement.area.x, placement.area.y + row))?;
            writer.write_all(&self.bands[usize::from(placement.first_row + row)])?;
        }
        Ok(())
    }
}
