//! Decode previews off the UI thread and retain at most eight thumbnail payloads.
use super::ImagePlacement;
use super::ImagePreview;
use super::MAX_FILE_BYTES;
use super::MAX_PIXELS;
use crate::tui::FrameRequester;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use crossterm::cursor::MoveTo;
use crossterm::cursor::RestorePosition;
use crossterm::cursor::SavePosition;
use crossterm::queue;
use image::ImageFormat;
use image::ImageReader;
use std::collections::VecDeque;
use std::io;
use std::io::Cursor;
use std::io::Read;
use std::io::Write;
use std::sync::Arc;
use std::sync::mpsc;
const MAX_CACHED_IMAGES: usize = 8;
pub(super) struct Pixels {
    width: u32,
    height: u32,
    transmission: String,
}
struct CachedImage {
    id: u32,
    source: Arc<ImagePreview>,
    pixels: Option<Arc<Pixels>>,
    uploaded: bool,
}
#[derive(Default)]
pub(crate) struct ImageRenderer {
    cache: VecDeque<CachedImage>,
    pending: Option<(Arc<ImagePreview>, mpsc::Receiver<Option<Arc<Pixels>>>)>,
    placed: Vec<u32>,
}
impl ImageRenderer {
    pub(crate) fn clear_placements(&mut self, writer: &mut impl Write) -> io::Result<()> {
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
                    Err(mpsc::TryRecvError::Disconnected) => Some((Arc::clone(source), None)),
                    Err(mpsc::TryRecvError::Empty) => None,
                });
        if let Some((source, pixels)) = completed {
            self.cache.push_back(CachedImage {
                id: source.id,
                source,
                pixels,
                uploaded: false,
            });
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
                    if !entry.uploaded {
                        writer.write_all(pixels.transmission.as_bytes())?;
                        entry.uploaded = true;
                    }
                    queue!(writer, MoveTo(placement.area.x, placement.area.y))?;
                    writer.write_all(placement_command(placement, pixels, entry.id).as_bytes())?;
                    self.placed.push(entry.id);
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
                let requester = requester.clone();
                let (sender, receiver) = mpsc::sync_channel(/*bound*/ 1);
                self.pending = Some((Arc::clone(&preview), receiver));
                tokio::task::spawn_blocking(move || {
                    let pixels = load_pixels(&preview).map(Arc::new);
                    preview
                        .failed
                        .store(pixels.is_none(), std::sync::atomic::Ordering::Relaxed);
                    let _ = sender.send(pixels);
                    requester.schedule_frame();
                });
            }
        }
        queue!(writer, RestorePosition)?;
        writer.flush()
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
    })
}

#[cfg(test)]
#[path = "renderer_tests.rs"]
mod tests;
