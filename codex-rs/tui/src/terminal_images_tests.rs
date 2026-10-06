use super::*;
use crate::history_cell::HistoryRenderMode;
use crate::history_cell::PlainHistoryCell;
use crate::transcript_view::TranscriptView;
use pretty_assertions::assert_eq;
use ratatui::buffer::Buffer;

fn preview(path: AbsolutePathBuf, width: u32, height: u32) -> Arc<ImagePreview> {
    Arc::new(ImagePreview {
        path,
        id: 7,
        failed: std::sync::atomic::AtomicBool::new(/*v*/ false),
        revision: crate::terminal_images::ImageRevision {
            bytes: 0,
            modified: None,
        },
        width,
        height,
    })
}

#[test]
fn preview_geometry_preserves_portrait_and_landscape_bounds() {
    let path = AbsolutePathBuf::try_from(std::env::temp_dir().join("preview.png")).unwrap();
    assert_eq!(
        preview(path.clone(), /*width*/ 400, /*height*/ 200).size(/*available_columns*/ 80),
        (40, 10)
    );
    assert_eq!(
        preview(path.clone(), /*width*/ 200, /*height*/ 400).size(/*available_columns*/ 80),
        (10, 10)
    );
    assert_eq!(
        preview(path, /*width*/ 400, /*height*/ 200).size(/*available_columns*/ 1),
        (1, 1)
    );
}

#[test]
fn image_rows_reflow_clip_and_stay_out_of_raw_text() {
    let path = AbsolutePathBuf::try_from(std::env::temp_dir().join("preview.png")).unwrap();
    let cell = Arc::new(ImageHistoryCell {
        cell: PlainHistoryCell::new(vec![Line::from("Viewed image preview.png")]),
        preview: Some(preview(path, /*width*/ 128, /*height*/ 64)),
    });
    let source = cell.raw_lines();
    let history: Vec<Arc<dyn HistoryCell>> = vec![cell];
    let mut view = TranscriptView::default();
    let mut snapshot = String::new();
    for area in [
        Rect::new(
            /*x*/ 0, /*y*/ 0, /*width*/ 24, /*height*/ 12,
        ),
        Rect::new(
            /*x*/ 0, /*y*/ 0, /*width*/ 40, /*height*/ 5,
        ),
    ] {
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf, &history);
        let placements = view.image_placements();
        assert_eq!(placements.len(), 1);
        let image = &placements[0];
        snapshot.push_str(&format!("Viewport {area:?}\n"));
        for y in area.top()..area.bottom() {
            let row = (area.left()..area.right())
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>();
            snapshot.push_str(&format!("|{row}|\n"));
        }
        snapshot.push_str(&format!(
            "Image {:?}, first row {}, total rows {}\n",
            image.area, image.first_row, image.total_rows
        ));
        assert!(image.area.bottom() <= area.bottom());
    }
    insta::assert_snapshot!(snapshot);
    view.set_presentation(/*detailed*/ false, HistoryRenderMode::Raw);
    let area = Rect::new(
        /*x*/ 0, /*y*/ 0, /*width*/ 40, /*height*/ 12,
    );
    view.render(area, &mut Buffer::empty(area), &history);
    assert!(view.image_placements().is_empty());
    assert_eq!(history[0].raw_lines(), source);
}

#[test]
fn invalid_and_oversized_files_keep_text_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invalid.png");
    std::fs::write(&path, b"not an image").unwrap();
    assert_eq!(dimensions(&path), None);
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(MAX_FILE_BYTES + 1).unwrap();
    assert_eq!(dimensions(&path), None);
}
