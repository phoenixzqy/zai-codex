//! Decode one preview at a time off the UI thread; retain at most eight thumbnail payloads.

use std::collections::VecDeque;
use std::io;
use std::io::Cursor;
use std::io::Read;
use std::io::Write;
use std::sync::Arc;
use std::sync::mpsc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use crossterm::cursor::MoveTo;
use crossterm::cursor::RestorePosition;
use crossterm::cursor::SavePosition;
use crossterm::queue;
use image::ImageFormat;
use image::ImageReader;

use super::ImagePlacement;
use super::ImagePreview;
use super::MAX_FILE_BYTES;
use super::MAX_PIXELS;
use crate::pets::ImageProtocol;
use crate::tui::FrameRequester;

pub(crate) mod raster;
use raster::RasterRows;

const MAX_CACHED_IMAGES: usize = 8;

pub(super) struct Pixels {
    width: u32,
    height: u32,
    transmission: String,
    raster: Arc<image::RgbaImage>,
}

struct CachedImage {
    id: u32,
    source: Arc<ImagePreview>,
    pixels: Option<Arc<Pixels>>,
    bands: Option<RasterRows>,
    uploaded: bool,
}

#[derive(Default)]
pub(crate) struct ImageRenderer {
    cache: VecDeque<CachedImage>,
    pending: Option<(Arc<ImagePreview>, mpsc::Receiver<PreparedImage>)>,
    protocol: Option<ImageProtocol>,
    raster_areas: Vec<ratatui::layout::Rect>,
    placed: Vec<u32>,
}

enum PreparedImage {
    Pixels(Option<Arc<Pixels>>),
    Bands(Option<RasterRows>),
}

impl ImageRenderer {
    /// Delete placements before text repaint, preserving terminal-side pixels for cheap reuse.
    pub(crate) fn clear_placements(&mut self, writer: &mut impl Write) -> io::Result<()> {
        if !self.raster_areas.is_empty() {
            queue!(writer, SavePosition)?;
            for area in self.raster_areas.drain(..) {
                for y in area.top()..area.bottom() {
                    queue!(writer, MoveTo(area.x, y))?;
                    write!(writer, "{}", " ".repeat(usize::from(area.width)))?;
                }
            }
            queue!(writer, RestorePosition)?;
        }
        for id in self.placed.drain(..) {
            write!(writer, "\x1b_Ga=d,d=i,i={id},q=2;\x1b\\")?;
        }
        Ok(())
    }

    pub(crate) fn clear(&mut self, writer: &mut impl Write) -> io::Result<()> {
        self.clear_placements(writer)?;
        for entry in self.cache.drain(..).filter(|entry| entry.uploaded) {
            write!(writer, "\x1b_Ga=d,d=I,i={},q=2;\x1b\\", entry.id)?;
        }
        Ok(())
    }

    pub(crate) fn draw(
        &mut self,
        writer: &mut impl Write,
        placements: &[ImagePlacement],
        requester: &FrameRequester,
    ) -> io::Result<()> {
        let completed =
            self.pending
                .as_ref()
                .and_then(|(source, receiver)| match receiver.try_recv() {
                    Ok(pixels) => Some((Arc::clone(source), pixels)),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Some((Arc::clone(source), PreparedImage::Pixels(None)))
                    }
                    Err(mpsc::TryRecvError::Empty) => None,
                });
        if let Some((source, prepared)) = completed {
            let id = source.id;
            match prepared {
                PreparedImage::Pixels(pixels) => self.cache.push_back(CachedImage {
                    id,
                    source,
                    pixels,
                    uploaded: false,
                    bands: None,
                }),
                PreparedImage::Bands(bands) => {
                    if let Some(entry) = self.cache.iter_mut().find(|entry| entry.id == id) {
                        entry.bands = bands;
                    }
                }
            }
            self.pending = None;
        }
        while self.cache.len() > MAX_CACHED_IMAGES {
            if let Some(entry) = self.cache.pop_front()
                && entry.uploaded
            {
                write!(writer, "\x1b_Ga=d,d=I,i={},q=2;\x1b\\", entry.id)?;
            }
        }
        if placements.is_empty() {
            return Ok(());
        }
        let protocol = self.protocol.or_else(super::protocol);
        let cell_size = raster::cell_size();
        queue!(writer, SavePosition)?;
        for placement in placements.iter().take(MAX_CACHED_IMAGES) {
            let cached = self
                .cache
                .iter()
                .position(|entry| {
                    entry.source.path == placement.preview.path
                        && entry.source.revision == placement.preview.revision
                })
                .and_then(|index| self.cache.remove(index));
            if let Some(mut entry) = cached {
                if let Some(pixels) = &entry.pixels {
                    if let Some(protocol @ (ImageProtocol::Sixel | ImageProtocol::KittyLocalFile)) =
                        protocol
                    {
                        let geometry = (placement.area.width, placement.total_rows, cell_size);
                        if let Some(rows) = entry
                            .bands
                            .as_ref()
                            .filter(|rows| rows.geometry == geometry)
                        {
                            rows.draw(writer, placement)?;
                            self.raster_areas.push(placement.area);
                        } else if self.pending.is_none() {
                            let raster = Arc::clone(&pixels.raster);
                            self.prepare(Arc::clone(&entry.source), requester, move || {
                                PreparedImage::Bands(RasterRows::new(&raster, geometry, protocol))
                            });
                        }
                    } else {
                        if !entry.uploaded {
                            writer.write_all(pixels.transmission.as_bytes())?;
                            entry.uploaded = true;
                        }
                        queue!(writer, MoveTo(placement.area.x, placement.area.y))?;
                        writer
                            .write_all(placement_command(placement, pixels, entry.id).as_bytes())?;
                        self.placed.push(entry.id);
                    }
                }
                if entry.pixels.is_none()
                    && !placement
                        .preview
                        .failed
                        .swap(true, std::sync::atomic::Ordering::Relaxed)
                {
                    requester.schedule_frame();
                }
                self.cache.push_back(entry);
            } else if self.pending.is_none() {
                let preview = Arc::clone(&placement.preview);
                self.prepare(Arc::clone(&preview), requester, move || {
                    let pixels = load_pixels(&preview).map(Arc::new);
                    preview
                        .failed
                        .store(pixels.is_none(), std::sync::atomic::Ordering::Relaxed);
                    PreparedImage::Pixels(pixels)
                });
            }
        }
        queue!(writer, RestorePosition)?;
        writer.flush()
    }

    fn prepare(
        &mut self,
        source: Arc<ImagePreview>,
        requester: &FrameRequester,
        work: impl FnOnce() -> PreparedImage + Send + 'static,
    ) {
        let requester = requester.clone();
        let (sender, receiver) = mpsc::sync_channel(/*bound*/ 1);
        self.pending = Some((source, receiver));
        tokio::task::spawn_blocking(move || {
            let _ = sender.send(work());
            requester.schedule_frame();
        });
    }
}

pub(super) fn placement_command(
    placement: &ImagePlacement,
    pixels: &Pixels,
    image_id: u32,
) -> String {
    let top = u32::from(placement.first_row) * pixels.height / u32::from(placement.total_rows);
    let bottom = u32::from(placement.first_row + placement.area.height) * pixels.height
        / u32::from(placement.total_rows);
    format!(
        "\x1b_Ga=p,i={image_id},p={},x=0,y={top},w={},h={},c={},r={},C=1,q=2;\x1b\\",
        placement.preview.id,
        pixels.width,
        (bottom - top).max(/*other*/ 1),
        placement.area.width,
        placement.area.height,
    )
}

pub(super) fn load_pixels(preview: &ImagePreview) -> Option<Pixels> {
    let file = std::fs::File::open(preview.path.as_path()).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return None;
    }
    let reader = || {
        ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .ok()
    };
    let (width, height) = reader()?.into_dimensions().ok()?;
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return None;
    }
    let mut reader = reader()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().ok()?;
    let image = image.thumbnail(
        image.width().min(/*other*/ 640),
        image.height().min(/*other*/ 320),
    );
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, ImageFormat::Png).ok()?;
    let payload = STANDARD.encode(png.into_inner());
    let mut transmission = String::new();
    let count = payload.len().div_ceil(/*rhs*/ 4096);
    for (index, chunk) in payload.as_bytes().chunks(/*chunk_size*/ 4096).enumerate() {
        let more = u8::from(index + 1 < count);
        let chunk = std::str::from_utf8(chunk).ok()?;
        if index == 0 {
            transmission.push_str(&format!(
                "\x1b_Ga=t,t=d,f=100,i={},q=2,m={more};{chunk}\x1b\\",
                preview.id,
            ));
        } else {
            transmission.push_str(&format!("\x1b_Gq=2,m={more};{chunk}\x1b\\"));
        }
    }
    Some(Pixels {
        width: image.width(),
        height: image.height(),
        transmission,
        raster: Arc::new(image.to_rgba8()),
    })
}

#[cfg(test)]
#[path = "renderer_tests.rs"]
mod tests;
