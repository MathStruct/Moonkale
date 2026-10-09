//! The proportional spike's only browser bridge. It measures grapheme ranges;
//! Rust maps geometry to source offsets and continues to own all editor state.
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Serialize)]
struct Segment {
    start: usize,
    end: usize,
    utf16_start: usize,
    utf16_end: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct GlyphBox {
    pub start: usize,
    pub end: usize,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Geometry {
    pub boxes: Vec<GlyphBox>,
    pub width: f64,
    pub height: f64,
}

impl Geometry {
    pub(crate) fn valid(&self, length: usize) -> bool {
        self.width.is_finite()
            && self.width > 0.0
            && self.height.is_finite()
            && self.height > 0.0
            && self.boxes.iter().all(|rect| {
                rect.start < rect.end
                    && rect.end <= length
                    && [rect.x, rect.y, rect.width, rect.height]
                        .iter()
                        .all(|v| v.is_finite())
                    && rect.width >= 0.0
                    && rect.height > 0.0
            })
    }

    /// Forward affinity places a wrapped boundary at the next visual row.
    pub(crate) fn caret(&self, offset: usize) -> Option<(f64, f64, f64)> {
        if self.boxes.is_empty() && offset == 0 {
            return Some((0.0, 0.0, self.height));
        }
        if let Some(rect) = self.boxes.iter().find(|rect| rect.start == offset) {
            return Some((rect.x, rect.y, rect.height));
        }
        self.boxes
            .iter()
            .rev()
            .find(|rect| rect.end == offset)
            .map(|rect| (rect.x + rect.width, rect.y, rect.height))
    }

    /// Backward affinity draws a wrap boundary after the preceding grapheme.
    pub(crate) fn caret_backward(&self, offset: usize) -> Option<(f64, f64, f64)> {
        self.boxes
            .iter()
            .rev()
            .find(|rect| rect.end == offset)
            .map(|rect| (rect.x + rect.width, rect.y, rect.height))
            .or_else(|| self.caret(offset))
    }

    pub(crate) fn hit(&self, x: f64, y: f64) -> Option<usize> {
        self.box_at(x, y).map(|rect| {
            if x >= rect.x + rect.width / 2.0 {
                rect.end
            } else {
                rect.start
            }
        })
    }

    pub(crate) fn box_at(&self, x: f64, y: f64) -> Option<&GlyphBox> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let distance =
            |point: f64, start: f64, size: f64| (start - point).max(0.0).max(point - start - size);
        self.boxes.iter().min_by(|a, b| {
            distance(y, a.y, a.height)
                .total_cmp(&distance(y, b.y, b.height))
                .then_with(|| distance(x, a.x, a.width).total_cmp(&distance(x, b.x, b.width)))
        })
    }
}

#[cfg(feature = "layout-fixture")]
pub(crate) async fn measure(id: &str, text: &str) -> Option<Geometry> {
    measure_with_status(id, text).await.ok()
}

#[cfg(feature = "layout-fixture")]
pub(crate) async fn measure_with_status(id: &str, text: &str) -> Result<Geometry, String> {
    measure_presented(id, text, &[]).await
}

pub(crate) async fn measure_presented(
    id: &str,
    text: &str,
    replacements: &[crate::native_presentation::Replacement],
) -> Result<Geometry, String> {
    let (mut scalar, mut utf16) = (0, 0);
    let segments: Vec<_> = text
        .graphemes(true)
        .map(|value| {
            let start = scalar;
            let utf16_start = utf16;
            scalar += value.chars().count();
            utf16 += value.encode_utf16().count();
            Segment {
                start,
                end: scalar,
                utf16_start,
                utf16_end: utf16,
            }
        })
        .collect();
    let mut eval = document::eval(include_str!("native_browser_geometry.js"));
    eval.send(serde_json::json!({"id": id, "text": text, "segments": segments, "replacements": replacements}))
        .map_err(|error| format!("send: {error}"))?;
    let received = eval.recv::<Option<Geometry>>().await;
    let _ = eval.send(serde_json::Value::Null);
    let result = received.map_err(|error| format!("receive: {error}"))?;
    let geometry = result.ok_or_else(|| "source-mismatch".to_string())?;
    if geometry.valid(scalar) {
        Ok(geometry)
    } else {
        Err(format!(
            "invalid-geometry: {}x{}, length {}, first invalid {:?}",
            geometry.width,
            geometry.height,
            scalar,
            geometry.boxes.iter().find(|rect| rect.start >= rect.end
                || rect.end > scalar
                || rect.height <= 0.0
                || rect.width < 0.0)
        ))
    }
}

#[derive(Deserialize)]
pub(crate) struct FontNotice {
    pub epoch: u64,
    pub loading: bool,
    pub closed: bool,
}

/// The JavaScript listener owns no editor state and closes on node removal.
/// The receiving task is also bound to the calling Dioxus scope.
pub(crate) async fn watch_fonts(id: &str, changed: Callback<FontNotice>) {
    let mut eval = document::eval(include_str!("native_font_watch.js"));
    if eval.send(serde_json::json!({"id":id})).is_err() {
        return;
    }
    while let Ok(notice) = eval.recv::<FontNotice>().await {
        if notice.closed {
            break;
        }
        changed.call(notice);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proportional_hits_use_measured_grapheme_boundaries_and_wrapped_affinity() {
        let geometry = Geometry {
            width: 20.0,
            height: 40.0,
            boxes: vec![
                GlyphBox {
                    start: 0,
                    end: 1,
                    x: 0.0,
                    y: 0.0,
                    width: 4.0,
                    height: 20.0,
                },
                GlyphBox {
                    start: 1,
                    end: 3,
                    x: 4.0,
                    y: 0.0,
                    width: 16.0,
                    height: 20.0,
                },
                GlyphBox {
                    start: 3,
                    end: 4,
                    x: 0.0,
                    y: 20.0,
                    width: 8.0,
                    height: 20.0,
                },
            ],
        };
        assert!(geometry.valid(4));
        assert_eq!(geometry.hit(2.1, 5.0), Some(1));
        assert_eq!(geometry.hit(10.0, 5.0), Some(1));
        assert_eq!(geometry.hit(15.0, 5.0), Some(3));
        assert_eq!(geometry.hit(4.1, 25.0), Some(4));
        assert_eq!(geometry.caret(3), Some((0.0, 20.0, 20.0)));
        assert_eq!(geometry.caret_backward(3), Some((20.0, 0.0, 20.0)));
        assert_eq!(geometry.caret_backward(0), Some((0.0, 0.0, 20.0)));
        assert_eq!(geometry.caret_backward(2), None);
        assert_eq!(geometry.caret(2), None);
        assert_eq!(geometry.hit(f64::NAN, 0.0), None);
        assert!(!geometry.valid(3));
    }
}
