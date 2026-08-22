//! `BMAP` 청크 — 명세 §5.7.

extern crate alloc;

use alloc::vec::Vec;

use crate::error::{Error, Result};

/// 비트맵 인코딩 — 명세 §5.7.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BitmapEncoding {
    RawRgba8 = 0,
    Png = 1,
    Webp = 2,
    Avif = 3,
}

impl BitmapEncoding {
    pub const fn from_u8(value: u8) -> Option<BitmapEncoding> {
        match value {
            0 => Some(BitmapEncoding::RawRgba8),
            1 => Some(BitmapEncoding::Png),
            2 => Some(BitmapEncoding::Webp),
            3 => Some(BitmapEncoding::Avif),
            _ => None,
        }
    }
}

/// 비트맵 본문 — `IBitmapPublishJson`.
#[derive(Debug, Clone, PartialEq)]
pub struct BitmapBody {
    pub bounds: [f32; 4],
    pub encoding: BitmapEncoding,
    pub data: Vec<u8>,
}

const HEADER_SIZE: usize = 24;

impl BitmapBody {
    pub fn parse(payload: &[u8]) -> Result<Self> {
        if payload.len() < HEADER_SIZE {
            return Err(Error::MalformedPayload("BMAP"));
        }

        let bounds = [
            f32::from_bits(u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]])),
            f32::from_bits(u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]])),
            f32::from_bits(u32::from_le_bytes([payload[8], payload[9], payload[10], payload[11]])),
            f32::from_bits(u32::from_le_bytes([
                payload[12], payload[13], payload[14], payload[15],
            ])),
        ];
        let encoding = BitmapEncoding::from_u8(payload[16]).ok_or(Error::MalformedPayload("BMAP"))?;
        let byte_len = u32::from_le_bytes([payload[20], payload[21], payload[22], payload[23]]) as usize;
        let end = HEADER_SIZE
            .checked_add(byte_len)
            .ok_or(Error::MalformedPayload("BMAP"))?;
        if end > payload.len() {
            return Err(Error::MalformedPayload("BMAP"));
        }

        Ok(BitmapBody {
            bounds,
            encoding,
            data: payload[HEADER_SIZE..end].to_vec(),
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_SIZE + self.data.len());
        for value in &self.bounds {
            out.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        out.push(self.encoding as u8);
        out.extend_from_slice(&[0u8; 3]);
        out.extend_from_slice(&(self.data.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.data);
        out
    }

    /// `BMAP` 청크 전체에서 `body_offset` 위치의 본문을 파싱한다.
    pub fn parse_at(bmap_payload: &[u8], body_offset: u32, body_size: u32) -> Result<Self> {
        let start = body_offset as usize;
        let end = start
            .checked_add(body_size as usize)
            .ok_or(Error::MalformedPayload("BMAP"))?;
        if end > bmap_payload.len() {
            return Err(Error::MalformedPayload("BMAP"));
        }
        Self::parse(&bmap_payload[start..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_rgba_round_trip() {
        let body = BitmapBody {
            bounds: [0.0, 64.0, 0.0, 48.0],
            encoding: BitmapEncoding::RawRgba8,
            data: vec![255, 0, 0, 255, 0, 255, 0, 255],
        };
        assert_eq!(BitmapBody::parse(&body.to_bytes()).unwrap(), body);
    }

    #[test]
    fn png_round_trip() {
        let body = BitmapBody {
            bounds: [0.0, 10.0, 0.0, 10.0],
            encoding: BitmapEncoding::Png,
            data: vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
        };
        assert_eq!(BitmapBody::parse(&body.to_bytes()).unwrap(), body);
    }

    #[test]
    fn parse_at_slice() {
        let body = BitmapBody {
            bounds: [0.0, 1.0, 0.0, 1.0],
            encoding: BitmapEncoding::Webp,
            data: vec![1, 2, 3],
        };
        let chunk = [0u8; 8]
            .into_iter()
            .chain(body.to_bytes())
            .collect::<Vec<_>>();
        assert_eq!(
            BitmapBody::parse_at(&chunk, 8, body.to_bytes().len() as u32).unwrap(),
            body
        );
    }

    #[test]
    fn rejects_unknown_encoding() {
        let mut bytes = BitmapBody {
            bounds: [0.0, 1.0, 0.0, 1.0],
            encoding: BitmapEncoding::RawRgba8,
            data: vec![],
        }
        .to_bytes();
        bytes[16] = 99;
        assert_eq!(
            BitmapBody::parse(&bytes),
            Err(Error::MalformedPayload("BMAP"))
        );
    }
}
