//! `MCLP` 청크 — 명세 §5.5.

extern crate alloc;

use alloc::vec::Vec;

use crate::error::{Error, Result};

/// 타임라인 사전 항목 — `ICharacterPublishObject`.
#[derive(Debug, Clone, PartialEq)]
pub struct DictionaryEntry {
    pub character_id: u32,
    pub start_frame: u32,
    pub end_frame: u32,
    pub clip_depth: i32,
}

/// 프레임에 배치되는 객체 — `IPlaceObject` (가변 길이).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlaceObject {
    pub matrix: Option<[f32; 6]>,
    pub color_transform: Option<[f32; 8]>,
    pub blend_mode: Option<u8>,
    pub filters: Vec<PlaceFilter>,
    pub r#loop: Option<LoopData>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaceFilter {
    pub class: u8,
    pub params: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoopData {
    pub loop_type: u8,
    pub start_frame: u32,
    pub frame_count: u32,
}

/// MovieClip 타임라인 본문.
#[derive(Debug, Clone, PartialEq)]
pub struct MovieClipBody {
    pub total_frame: u32,
    pub dictionary: Vec<DictionaryEntry>,
    pub depth_count: u32,
    pub place_objects: Vec<PlaceObject>,
    /// `depth_count × total_frame`, `-1` = null.
    pub controller: Vec<i32>,
    /// `depth_count × total_frame`, `-1` = null.
    pub place_map: Vec<i32>,
}

impl MovieClipBody {
    pub fn parse(payload: &[u8]) -> Result<Self> {
        let mut offset = 0usize;
        let total_frame = read_u32(payload, &mut offset)?;
        let dictionary_count = read_u32(payload, &mut offset)?;

        let mut dictionary = Vec::with_capacity(dictionary_count as usize);
        for _ in 0..dictionary_count {
            dictionary.push(DictionaryEntry {
                character_id: read_u32(payload, &mut offset)?,
                start_frame: read_u32(payload, &mut offset)?,
                end_frame: read_u32(payload, &mut offset)?,
                clip_depth: read_i32(payload, &mut offset)?,
            });
        }

        let depth_count = read_u32(payload, &mut offset)?;
        let place_object_count = read_u32(payload, &mut offset)?;

        let mut place_objects = Vec::with_capacity(place_object_count as usize);
        for _ in 0..place_object_count {
            place_objects.push(parse_place_object(payload, &mut offset)?);
        }

        let controller_len = read_u32(payload, &mut offset)?;
        let expected_controller = depth_count as usize * total_frame as usize;
        if controller_len as usize != expected_controller {
            return Err(Error::MalformedPayload("MCLP"));
        }

        let mut controller = Vec::with_capacity(controller_len as usize);
        for _ in 0..controller_len {
            controller.push(read_i32(payload, &mut offset)?);
        }

        let place_map_len = read_u32(payload, &mut offset)?;
        if place_map_len as usize != expected_controller {
            return Err(Error::MalformedPayload("MCLP"));
        }

        let mut place_map = Vec::with_capacity(place_map_len as usize);
        for _ in 0..place_map_len {
            place_map.push(read_i32(payload, &mut offset)?);
        }

        Ok(MovieClipBody {
            total_frame,
            dictionary,
            depth_count,
            place_objects,
            controller,
            place_map,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.total_frame.to_le_bytes());
        out.extend_from_slice(&(self.dictionary.len() as u32).to_le_bytes());
        for entry in &self.dictionary {
            out.extend_from_slice(&entry.character_id.to_le_bytes());
            out.extend_from_slice(&entry.start_frame.to_le_bytes());
            out.extend_from_slice(&entry.end_frame.to_le_bytes());
            out.extend_from_slice(&entry.clip_depth.to_le_bytes());
        }
        out.extend_from_slice(&self.depth_count.to_le_bytes());
        out.extend_from_slice(&(self.place_objects.len() as u32).to_le_bytes());
        for place in &self.place_objects {
            out.extend_from_slice(&encode_place_object(place));
        }
        let matrix_len = (self.depth_count as usize * self.total_frame as usize) as u32;
        out.extend_from_slice(&matrix_len.to_le_bytes());
        for value in &self.controller {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(&matrix_len.to_le_bytes());
        for value in &self.place_map {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out
    }

    /// `MCLP` 청크 전체에서 `body_offset` 위치의 본문을 파싱한다.
    pub fn parse_at(mclp_payload: &[u8], body_offset: u32, body_size: u32) -> Result<Self> {
        let start = body_offset as usize;
        let end = start
            .checked_add(body_size as usize)
            .ok_or(Error::MalformedPayload("MCLP"))?;
        if end > mclp_payload.len() {
            return Err(Error::MalformedPayload("MCLP"));
        }
        Self::parse(&mclp_payload[start..end])
    }
}

fn read_u32(payload: &[u8], offset: &mut usize) -> Result<u32> {
    if *offset + 4 > payload.len() {
        return Err(Error::MalformedPayload("MCLP"));
    }
    let value = u32::from_le_bytes([
        payload[*offset],
        payload[*offset + 1],
        payload[*offset + 2],
        payload[*offset + 3],
    ]);
    *offset += 4;
    Ok(value)
}

fn read_i32(payload: &[u8], offset: &mut usize) -> Result<i32> {
    Ok(read_u32(payload, offset)? as i32)
}

const PRESENT_MATRIX: u8 = 1 << 0;
const PRESENT_COLOR: u8 = 1 << 1;
const PRESENT_BLEND: u8 = 1 << 2;
const PRESENT_FILTERS: u8 = 1 << 3;
const PRESENT_LOOP: u8 = 1 << 4;

fn parse_place_object(payload: &[u8], offset: &mut usize) -> Result<PlaceObject> {
    if *offset + 2 > payload.len() {
        return Err(Error::MalformedPayload("MCLP"));
    }
    let present = payload[*offset];
    *offset += 1;
    let blend_mode = if present & PRESENT_BLEND != 0 {
        Some(payload[*offset])
    } else {
        None
    };
    *offset += 1;
    let filter_count = if present & PRESENT_FILTERS != 0 {
        u16::from_le_bytes([payload[*offset], payload[*offset + 1]])
    } else {
        0
    };
    if present & PRESENT_FILTERS != 0 {
        *offset += 2;
    }

    let matrix = if present & PRESENT_MATRIX != 0 {
        Some(read_f32_array::<6>(payload, offset)?)
    } else {
        None
    };

    let color_transform = if present & PRESENT_COLOR != 0 {
        Some(read_f32_array::<8>(payload, offset)?)
    } else {
        None
    };

    let mut filters = Vec::new();
    for _ in 0..filter_count {
        if *offset + 4 > payload.len() {
            return Err(Error::MalformedPayload("MCLP"));
        }
        let class = payload[*offset];
        let param_count = payload[*offset + 1] as usize;
        *offset += 4;
        let mut params = Vec::with_capacity(param_count);
        for _ in 0..param_count {
            params.push(read_f32(payload, offset)?);
        }
        filters.push(PlaceFilter { class, params });
    }

    let r#loop = if present & PRESENT_LOOP != 0 {
        if *offset + 12 > payload.len() {
            return Err(Error::MalformedPayload("MCLP"));
        }
        let loop_type = payload[*offset];
        *offset += 4;
        let start_frame = read_u32(payload, offset)?;
        let frame_count = read_u32(payload, offset)?;
        Some(LoopData {
            loop_type,
            start_frame,
            frame_count,
        })
    } else {
        None
    };

    Ok(PlaceObject {
        matrix,
        color_transform,
        blend_mode,
        filters,
        r#loop,
    })
}

fn read_f32(payload: &[u8], offset: &mut usize) -> Result<f32> {
    if *offset + 4 > payload.len() {
        return Err(Error::MalformedPayload("MCLP"));
    }
    let value = f32::from_bits(u32::from_le_bytes([
        payload[*offset],
        payload[*offset + 1],
        payload[*offset + 2],
        payload[*offset + 3],
    ]));
    *offset += 4;
    Ok(value)
}

fn read_f32_array<const N: usize>(payload: &[u8], offset: &mut usize) -> Result<[f32; N]> {
    let mut out = [0.0f32; N];
    for slot in &mut out {
        *slot = read_f32(payload, offset)?;
    }
    Ok(out)
}

fn encode_place_object(place: &PlaceObject) -> Vec<u8> {
    let mut present = 0u8;
    if place.matrix.is_some() {
        present |= PRESENT_MATRIX;
    }
    if place.color_transform.is_some() {
        present |= PRESENT_COLOR;
    }
    if place.blend_mode.is_some() {
        present |= PRESENT_BLEND;
    }
    if !place.filters.is_empty() {
        present |= PRESENT_FILTERS;
    }
    if place.r#loop.is_some() {
        present |= PRESENT_LOOP;
    }

    let mut out = Vec::new();
    out.push(present);
    out.push(place.blend_mode.unwrap_or(0));
    if present & PRESENT_FILTERS != 0 {
        out.extend_from_slice(&(place.filters.len() as u16).to_le_bytes());
    }

    if let Some(matrix) = &place.matrix {
        for value in matrix {
            out.extend_from_slice(&value.to_bits().to_le_bytes());
        }
    }
    if let Some(color) = &place.color_transform {
        for value in color {
            out.extend_from_slice(&value.to_bits().to_le_bytes());
        }
    }
    for filter in &place.filters {
        out.push(filter.class);
        out.push(filter.params.len() as u8);
        out.extend_from_slice(&0u16.to_le_bytes());
        for param in &filter.params {
            out.extend_from_slice(&param.to_bits().to_le_bytes());
        }
    }
    if let Some(r#loop) = &place.r#loop {
        out.push(r#loop.loop_type);
        out.extend_from_slice(&[0u8; 3]);
        out.extend_from_slice(&r#loop.start_frame.to_le_bytes());
        out.extend_from_slice(&r#loop.frame_count.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_timeline_round_trip() {
        let body = MovieClipBody {
            total_frame: 1,
            dictionary: vec![],
            depth_count: 0,
            place_objects: vec![],
            controller: vec![],
            place_map: vec![],
        };
        assert_eq!(MovieClipBody::parse(&body.to_bytes()).unwrap(), body);
    }

    #[test]
    fn place_object_with_matrix() {
        let body = MovieClipBody {
            total_frame: 2,
            dictionary: vec![DictionaryEntry {
                character_id: 1,
                start_frame: 0,
                end_frame: 1,
                clip_depth: -1,
            }],
            depth_count: 1,
            place_objects: vec![PlaceObject {
                matrix: Some([1.0, 0.0, 0.0, 1.0, 10.0, 20.0]),
                ..Default::default()
            }],
            controller: vec![0, -1],
            place_map: vec![0, -1],
        };
        let bytes = body.to_bytes();
        assert_eq!(MovieClipBody::parse(&bytes).unwrap(), body);
    }

    #[test]
    fn parse_at_slice() {
        let body = MovieClipBody {
            total_frame: 1,
            dictionary: vec![],
            depth_count: 0,
            place_objects: vec![],
            controller: vec![],
            place_map: vec![],
        };
        let chunk = [0u8; 8].into_iter().chain(body.to_bytes()).collect::<Vec<_>>();
        assert_eq!(
            MovieClipBody::parse_at(&chunk, 8, body.to_bytes().len() as u32).unwrap(),
            body
        );
    }
}
