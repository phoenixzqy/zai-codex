//! Bounded, host-local image previews. Pixels belong to the terminal, never copied transcript text.

mod renderer;
pub(crate) use renderer::ImageRenderer;
#[cfg(unix)]
pub(crate) use renderer::raster::cell_size_report;
#[cfg(unix)]
pub(crate) use renderer::raster::observe_cell_size;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;

use codex_utils_absolute_path::AbsolutePathBuf;
use image::ImageReader;
use ratatui::layout::Rect;
use ratatui::text::Line;

use crate::history_cell::HistoryCell;

const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_PIXELS: u64 = 16 * 1024 * 1024;
static NEXT_ID: AtomicU32 = AtomicU32::new(/*v*/ 0x4300_0000);

#[derive(Debug, Eq, PartialEq)]
struct ImageRevision {
    bytes: u64,
    modified: Option<std::time::SystemTime>,
}

#[derive(Debug)]
pub(crate) struct ImagePreview {
    pub(crate) path: AbsolutePathBuf,
    pub(crate) id: u32,
    failed: AtomicBool,
    revision: ImageRevision,
    width: u32,
    height: u32,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum ImageProtocol {
    Kitty,
    Sixel,
    Iterm,
}

pub(crate) fn protocol() -> Option<ImageProtocol> {
    if std::env::var_os("CODEX_TUI_IMAGE_PREVIEWS").as_deref() == Some(std::ffi::OsStr::new("0")) {
        return None;
    }
    use crate::pets::ImageProtocol as PetProtocol;
    use crate::pets::PetImageSupport;
    use crate::pets::PetImageUnsupportedReason;
    match crate::pets::detect_pet_image_support() {
        PetImageSupport::Supported(PetProtocol::Kitty) => Some(ImageProtocol::Kitty),
        PetImageSupport::Supported(PetProtocol::Sixel) => Some(ImageProtocol::Sixel),
        PetImageSupport::Supported(PetProtocol::KittyLocalFile)
        | PetImageSupport::Unsupported(PetImageUnsupportedReason::Iterm2TooOld) => {
            Some(ImageProtocol::Iterm)
        }
        PetImageSupport::Unsupported(
            PetImageUnsupportedReason::Tmux
            | PetImageUnsupportedReason::Zellij
            | PetImageUnsupportedReason::Terminal,
        ) => None,
    }
}

impl ImagePreview {
    pub(crate) fn new(path: AbsolutePathBuf) -> Option<Arc<Self>> {
        protocol()?;
        let (width, height, revision) = dimensions(path.as_path())?;
        Some(Arc::new(Self {
            path,
            id: NEXT_ID.fetch_add(/*val*/ 1, Ordering::Relaxed),
            width,
            height,
            failed: AtomicBool::new(/*v*/ false),
            revision,
        }))
    }

    /// Approximate terminal cells as twice as tall as they are wide; cap the reserved area.
    pub(crate) fn size(&self, available_columns: u16) -> (u16, u16) {
        let columns = available_columns
            .saturating_sub(/*rhs*/ 2)
            .clamp(/*min*/ 1, /*max*/ 48);
        let rows = (u64::from(columns) * u64::from(self.height))
            .div_ceil(2 * u64::from(self.width))
            .clamp(/*min*/ 1, /*max*/ 10) as u16;
        let columns = columns.min(
            (2 * u64::from(rows) * u64::from(self.width))
                .div_ceil(u64::from(self.height))
                .clamp(/*min*/ 1, u64::from(columns)) as u16,
        );
        (columns, rows)
    }
}

fn dimensions(path: &Path) -> Option<(u32, u32, ImageRevision)> {
    let metadata = path.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return None;
    }
    let (width, height) = ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    (width > 0 && height > 0 && u64::from(width) * u64::from(height) <= MAX_PIXELS).then_some((
        width,
        height,
        ImageRevision {
            bytes: metadata.len(),
            modified: metadata.modified().ok(),
        },
    ))
}

#[derive(Clone, Debug)]
pub(crate) struct ImagePlacement {
    pub(crate) preview: Arc<ImagePreview>,
    pub(crate) area: Rect,
    pub(crate) first_row: u16,
    pub(crate) total_rows: u16,
}

/// Adds terminal-only metadata while preserving the existing tool's textual presentation.
#[derive(Debug)]
pub(crate) struct ImageHistoryCell<T> {
    cell: T,
    preview: Option<Arc<ImagePreview>>,
}

impl<T> ImageHistoryCell<T> {
    pub(crate) fn new(cell: T, path: Option<AbsolutePathBuf>) -> Self {
        Self {
            cell,
            preview: path.and_then(ImagePreview::new),
        }
    }
}

impl<T: HistoryCell> HistoryCell for ImageHistoryCell<T> {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        self.cell.display_lines(width)
    }

    fn transcript_lines(&self, width: u16) -> Vec<Line<'static>> {
        self.cell.transcript_lines(width)
    }

    fn raw_lines(&self) -> Vec<Line<'static>> {
        self.cell.raw_lines()
    }

    fn transcript_animation_tick(&self) -> Option<u64> {
        self.preview
            .as_ref()
            .map(|preview| u64::from(preview.failed.load(Ordering::Relaxed)))
    }

    fn image_preview(&self) -> Option<Arc<ImagePreview>> {
        self.preview
            .clone()
            .filter(|preview| !preview.failed.load(Ordering::Relaxed))
    }
}

#[cfg(test)]
#[path = "terminal_images_tests.rs"]
mod tests;
