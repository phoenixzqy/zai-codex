use super::*;
use codex_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use ratatui::layout::Rect;

fn image_preview(dir: &Path, id: u32) -> ImagePreview {
    ImagePreview {
        path: AbsolutePathBuf::try_from(dir.join("image.png")).unwrap(),
        id,
        failed: std::sync::atomic::AtomicBool::new(/*v*/ false),
        revision: crate::terminal_images::ImageRevision {
            bytes: 0,
            modified: None,
        },
        width: 128,
        height: 64,
    }
}

use std::path::Path;

#[tokio::test]
async fn redraw_reuses_uploaded_pixels_and_deletes_only_own_images() {
    let dir = tempfile::tempdir().unwrap();
    let preview = Arc::new(image_preview(dir.path(), /*id*/ 9));
    image::RgbaImage::from_pixel(
        /*width*/ 128,
        /*height*/ 64,
        image::Rgba([255, 0, 0, 255]),
    )
    .save(preview.path.as_path())
    .unwrap();
    let pixels = load_pixels(&preview).unwrap();
    let transmission = pixels.transmission.clone();
    let mut renderer = ImageRenderer::default();
    renderer.cache.push_back(CachedImage {
        id: preview.id,
        source: Arc::clone(&preview),
        pixels: Some(Arc::new(pixels)),
        uploaded: false,
    });
    let (tx, _) = tokio::sync::broadcast::channel(/*capacity*/ 1);
    let requester = FrameRequester::new(tx);
    let placement = ImagePlacement {
        preview,
        area: Rect::new(
            /*x*/ 1, /*y*/ 4, /*width*/ 20, /*height*/ 3,
        ),
        first_row: 2,
        total_rows: 8,
    };
    let mut output = Vec::new();
    renderer
        .draw(&mut output, std::slice::from_ref(&placement), &requester)
        .unwrap();
    assert!(
        String::from_utf8(output.clone())
            .unwrap()
            .contains(&transmission)
    );
    output.clear();
    renderer.clear_placements(&mut output).unwrap();
    let replacement = ImagePlacement {
        preview: Arc::new(image_preview(dir.path(), /*id*/ 10)),
        ..placement
    };
    renderer
        .draw(&mut output, &[replacement], &requester)
        .unwrap();
    let redraw = String::from_utf8(output).unwrap();
    assert!(!redraw.contains(&transmission));
    assert!(redraw.contains("a=d,d=i,i=9"));
    assert!(redraw.contains("a=p,i=9,p=10,x=0,y=16,w=128,h=24,c=20,r=3,C=1,q=2"));
    let mut output = Vec::new();
    renderer.clear(&mut output).unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "\x1b_Ga=d,d=i,i=9,q=2;\x1b\\\x1b_Ga=d,d=I,i=9,q=2;\x1b\\"
    );
}

#[test]
fn corrupt_pixels_never_reach_terminal_and_thumbnail_is_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let preview = image_preview(dir.path(), /*id*/ 10);
    std::fs::write(preview.path.as_path(), b"not an image").unwrap();
    assert!(load_pixels(&preview).is_none());
    image::RgbImage::new(/*width*/ 2000, /*height*/ 1000)
        .save(preview.path.as_path())
        .unwrap();
    let pixels = load_pixels(&preview).unwrap();
    assert_eq!((pixels.width, pixels.height), (640, 320));
    assert!(pixels.transmission.len() < 2 * 1024 * 1024);
}
