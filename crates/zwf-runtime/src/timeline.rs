//! MovieClip 타임라인 평가 — 프레임 N 의 표시 목록을 만든다.
//!
//! `controller` / `place_map` 은 `depth × frame` 밀집 행렬이며 셀 인덱스는
//! `depth * total_frame + frame` 이다 (에디터 `flattenFrameMajor` 와 동일).

extern crate alloc;

use alloc::vec::Vec;

use zwf_format::{DictionaryEntry, MovieClipBody, PlaceObject};

/// 렌더 목록 한 항목의 `f32` 개수. JS `Float32Array` 뷰와 1:1.
pub const RENDER_ITEM_FLOATS: usize = 17;

/// 항목 레이아웃: `[depth, character_id, matrix×6, color×8, blend_mode]`.
pub const RENDER_ITEM_DEPTH: usize = 0;
pub const RENDER_ITEM_CHARACTER_ID: usize = 1;
pub const RENDER_ITEM_MATRIX: usize = 2;
pub const RENDER_ITEM_COLOR: usize = 8;
pub const RENDER_ITEM_BLEND: usize = 16;

pub const IDENTITY_MATRIX: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
pub const IDENTITY_COLOR_TRANSFORM: [f32; 8] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

/// 한 depth 에 배치된 표시 객체.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderItem {
    pub depth: u32,
    pub character_id: u32,
    pub matrix: [f32; 6],
    pub color_transform: [f32; 8],
    pub blend_mode: u8,
}

/// 프레임 `frame` 의 표시 목록을 depth 오름차순으로 만든다.
pub fn evaluate_frame(body: &MovieClipBody, frame: u32) -> Result<Vec<RenderItem>, FrameError> {
    if frame >= body.total_frame {
        return Err(FrameError::OutOfRange);
    }

    let mut items = Vec::new();

    for depth in 0..body.depth_count {
        let cell = depth as usize * body.total_frame as usize + frame as usize;
        let dict_idx = body.controller[cell];
        if dict_idx < 0 {
            continue;
        }

        let dict_idx = dict_idx as usize;
        let Some(entry) = body.dictionary.get(dict_idx) else {
            continue;
        };

        if !dictionary_active(entry, frame) {
            continue;
        }

        let place = place_object_at(body, cell);
        items.push(RenderItem {
            depth,
            character_id: entry.character_id,
            matrix: place
                .and_then(|object| object.matrix)
                .unwrap_or(IDENTITY_MATRIX),
            color_transform: place
                .and_then(|object| object.color_transform)
                .unwrap_or(IDENTITY_COLOR_TRANSFORM),
            blend_mode: place.and_then(|object| object.blend_mode).unwrap_or(0),
        });
    }

    Ok(items)
}

/// `out` 에 항목을 직렬화한다. 반환값은 쓴 `f32` 개수.
pub fn write_render_items(items: &[RenderItem], out: &mut [f32]) -> Result<usize, FrameError> {
    let needed = items.len() * RENDER_ITEM_FLOATS;
    if out.len() < needed {
        return Err(FrameError::BufferTooSmall {
            needed_floats: needed,
        });
    }

    for (index, item) in items.iter().enumerate() {
        let base = index * RENDER_ITEM_FLOATS;
        out[base + RENDER_ITEM_DEPTH] = item.depth as f32;
        out[base + RENDER_ITEM_CHARACTER_ID] = item.character_id as f32;
        for (offset, value) in item.matrix.iter().enumerate() {
            out[base + RENDER_ITEM_MATRIX + offset] = *value;
        }
        for (offset, value) in item.color_transform.iter().enumerate() {
            out[base + RENDER_ITEM_COLOR + offset] = *value;
        }
        out[base + RENDER_ITEM_BLEND] = item.blend_mode as f32;
    }

    Ok(needed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    OutOfRange,
    BufferTooSmall { needed_floats: usize },
}

fn place_object_at<'a>(body: &'a MovieClipBody, cell: usize) -> Option<&'a PlaceObject> {
    let place_idx = *body.place_map.get(cell)?;
    if place_idx < 0 {
        return None;
    }
    body.place_objects.get(place_idx as usize)
}

/// Next2D `MovieClipGetChildrenService` 와 같은 활성 구간 규칙.
fn dictionary_active(entry: &DictionaryEntry, frame: u32) -> bool {
    (entry.start_frame == 1 && entry.end_frame == 0)
        || (entry.start_frame <= frame && entry.end_frame > frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zwf_format::{DictionaryEntry, MovieClipBody, PlaceObject};

    fn sample_body() -> MovieClipBody {
        MovieClipBody {
            total_frame: 2,
            dictionary: vec![DictionaryEntry {
                character_id: 7,
                start_frame: 0,
                end_frame: 2,
                clip_depth: -1,
            }],
            depth_count: 1,
            place_objects: vec![PlaceObject {
                matrix: Some([1.0, 0.0, 0.0, 1.0, 100.0, 50.0]),
                color_transform: Some([1.0, 0.5, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0]),
                blend_mode: Some(2),
                ..Default::default()
            }],
            controller: vec![0, -1],
            place_map: vec![0, -1],
        }
    }

    #[test]
    fn evaluates_active_depth_on_frame_zero() {
        let items = evaluate_frame(&sample_body(), 0).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].depth, 0);
        assert_eq!(items[0].character_id, 7);
        assert_eq!(items[0].matrix, [1.0, 0.0, 0.0, 1.0, 100.0, 50.0]);
        assert_eq!(
            items[0].color_transform,
            [1.0, 0.5, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0]
        );
        assert_eq!(items[0].blend_mode, 2);
    }

    #[test]
    fn skips_empty_controller_cells() {
        let items = evaluate_frame(&sample_body(), 1).unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn rejects_out_of_range_frame() {
        assert_eq!(
            evaluate_frame(&sample_body(), 2),
            Err(FrameError::OutOfRange)
        );
    }

    #[test]
    fn writes_render_items_to_buffer() {
        let items = evaluate_frame(&sample_body(), 0).unwrap();
        let mut out = [0.0f32; RENDER_ITEM_FLOATS];
        write_render_items(&items, &mut out).unwrap();
        assert_eq!(out[RENDER_ITEM_DEPTH], 0.0);
        assert_eq!(out[RENDER_ITEM_CHARACTER_ID], 7.0);
        assert_eq!(out[RENDER_ITEM_MATRIX + 4], 100.0);
        assert_eq!(out[RENDER_ITEM_COLOR + 1], 0.5);
        assert_eq!(out[RENDER_ITEM_BLEND], 2.0);
    }
}
