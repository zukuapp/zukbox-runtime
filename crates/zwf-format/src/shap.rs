//! `SHAP` 청크 — 명세 §5.6.

extern crate alloc;

use alloc::vec::Vec;

use crate::error::{Error, Result};

pub const FLAG_HAS_GRID: u8 = 1 << 0;
pub const FLAG_IN_BITMAP: u8 = 1 << 1;
pub const FLAG_HAS_BITMAP_ID: u8 = 1 << 2;

/// 벡터 셰이프 본문 — `IShapePublishJson`.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeBody {
    pub bounds: [f32; 4],
    pub in_bitmap: bool,
    pub grid: Option<[f32; 4]>,
    pub bitmap_id: Option<u32>,
    pub recodes: Vec<f32>,
}

impl ShapeBody {
    pub fn parse(payload: &[u8]) -> Result<Self> {
        let mut offset = 0usize;
        let bounds = read_f32_array::<4>(payload, &mut offset, "SHAP")?;
        if offset + 4 > payload.len() {
            return Err(Error::MalformedPayload("SHAP"));
        }
        let flags = payload[offset];
        offset += 4;

        let in_bitmap = flags & FLAG_IN_BITMAP != 0;
        let grid = if flags & FLAG_HAS_GRID != 0 {
            Some(read_f32_array::<4>(payload, &mut offset, "SHAP")?)
        } else {
            None
        };
        let bitmap_id = if flags & FLAG_HAS_BITMAP_ID != 0 {
            Some(read_u32(payload, &mut offset, "SHAP")?)
        } else {
            None
        };

        let recode_len = read_u32(payload, &mut offset, "SHAP")? as usize;
        let recode_bytes = recode_len
            .checked_mul(4)
            .ok_or(Error::MalformedPayload("SHAP"))?;
        if offset + recode_bytes > payload.len() {
            return Err(Error::MalformedPayload("SHAP"));
        }

        let mut recodes = Vec::with_capacity(recode_len);
        for _ in 0..recode_len {
            recodes.push(read_f32(payload, &mut offset, "SHAP")?);
        }

        Ok(ShapeBody {
            bounds,
            in_bitmap,
            grid,
            bitmap_id,
            recodes,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut flags = 0u8;
        if self.in_bitmap {
            flags |= FLAG_IN_BITMAP;
        }
        if self.grid.is_some() {
            flags |= FLAG_HAS_GRID;
        }
        if self.bitmap_id.is_some() {
            flags |= FLAG_HAS_BITMAP_ID;
        }

        let mut out = Vec::new();
        for value in &self.bounds {
            out.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        out.push(flags);
        out.extend_from_slice(&[0u8; 3]);
        if let Some(grid) = &self.grid {
            for value in grid {
                out.extend_from_slice(&value.to_bits().to_le_bytes());
            }
        }
        if let Some(bitmap_id) = self.bitmap_id {
            out.extend_from_slice(&bitmap_id.to_le_bytes());
        }
        out.extend_from_slice(&(self.recodes.len() as u32).to_le_bytes());
        for value in &self.recodes {
            out.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        out
    }

    /// `SHAP` 청크 전체에서 `body_offset` 위치의 본문을 파싱한다.
    pub fn parse_at(shap_payload: &[u8], body_offset: u32, body_size: u32) -> Result<Self> {
        let start = body_offset as usize;
        let end = start
            .checked_add(body_size as usize)
            .ok_or(Error::MalformedPayload("SHAP"))?;
        if end > shap_payload.len() {
            return Err(Error::MalformedPayload("SHAP"));
        }
        Self::parse(&shap_payload[start..end])
    }
}

fn read_u32(payload: &[u8], offset: &mut usize, chunk: &'static str) -> Result<u32> {
    if *offset + 4 > payload.len() {
        return Err(Error::MalformedPayload(chunk));
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

fn read_f32(payload: &[u8], offset: &mut usize, chunk: &'static str) -> Result<f32> {
    Ok(f32::from_bits(read_u32(payload, offset, chunk)?))
}

fn read_f32_array<const N: usize>(
    payload: &[u8],
    offset: &mut usize,
    chunk: &'static str,
) -> Result<[f32; N]> {
    let mut out = [0.0f32; N];
    for slot in &mut out {
        *slot = read_f32(payload, offset, chunk)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_round_trip() {
        let body = ShapeBody {
            bounds: [0.0, 100.0, 0.0, 50.0],
            in_bitmap: false,
            grid: None,
            bitmap_id: None,
            recodes: vec![1.0, 2.0, 3.0],
        };
        assert_eq!(ShapeBody::parse(&body.to_bytes()).unwrap(), body);
    }

    #[test]
    fn optional_fields_round_trip() {
        let body = ShapeBody {
            bounds: [-10.0, 10.0, -5.0, 5.0],
            in_bitmap: true,
            grid: Some([1.0, 2.0, 3.0, 4.0]),
            bitmap_id: Some(7),
            recodes: vec![0.5],
        };
        assert_eq!(ShapeBody::parse(&body.to_bytes()).unwrap(), body);
    }

    #[test]
    fn parse_at_slice() {
        let body = ShapeBody {
            bounds: [0.0, 1.0, 0.0, 1.0],
            in_bitmap: false,
            grid: None,
            bitmap_id: None,
            recodes: vec![],
        };
        let chunk = [0u8; 4]
            .into_iter()
            .chain(body.to_bytes())
            .collect::<Vec<_>>();
        assert_eq!(
            ShapeBody::parse_at(&chunk, 4, body.to_bytes().len() as u32).unwrap(),
            body
        );
    }

    #[test]
    fn rejects_truncated_payload() {
        let body = ShapeBody {
            bounds: [0.0, 1.0, 0.0, 1.0],
            in_bitmap: false,
            grid: None,
            bitmap_id: None,
            recodes: vec![1.0],
        };
        let bytes = body.to_bytes();
        assert_eq!(
            ShapeBody::parse(&bytes[..bytes.len() - 1]),
            Err(Error::MalformedPayload("SHAP"))
        );
    }
}
