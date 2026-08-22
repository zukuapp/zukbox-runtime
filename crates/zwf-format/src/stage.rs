//! `STAG` 청크 — 명세 §5.2.

use crate::error::{Error, Result};

/// `STAG` 페이로드 크기. 고정이다.
pub const STAGE_SIZE: usize = 24;

/// 재생을 시작하려면 이것만 있으면 된다. 캔버스 크기·배경·틱 주기가 전부 여기 있다.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stage {
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    /// `0xRRGGBBAA`.
    pub bg_rgba: u32,
    pub root_character_id: u32,
}

impl Stage {
    pub fn parse(payload: &[u8]) -> Result<Self> {
        if payload.len() < STAGE_SIZE {
            return Err(Error::MalformedPayload("STAG"));
        }

        let read_u32 = |offset: usize| {
            u32::from_le_bytes([
                payload[offset],
                payload[offset + 1],
                payload[offset + 2],
                payload[offset + 3],
            ])
        };

        Ok(Stage {
            width: read_u32(0),
            height: read_u32(4),
            fps: f32::from_bits(read_u32(8)),
            bg_rgba: read_u32(12),
            root_character_id: read_u32(16),
        })
    }

    pub fn to_bytes(&self) -> [u8; STAGE_SIZE] {
        let mut out = [0u8; STAGE_SIZE];
        out[0..4].copy_from_slice(&self.width.to_le_bytes());
        out[4..8].copy_from_slice(&self.height.to_le_bytes());
        out[8..12].copy_from_slice(&self.fps.to_bits().to_le_bytes());
        out[12..16].copy_from_slice(&self.bg_rgba.to_le_bytes());
        out[16..20].copy_from_slice(&self.root_character_id.to_le_bytes());
        // 20..24 는 reserved, 0 유지.
        out
    }

    /// 한 프레임의 길이(밀리초). `fps` 가 0 이하이면 재생이 멈추지 않도록 60fps 로 본다.
    pub fn frame_duration_ms(&self) -> f32 {
        if self.fps > 0.0 {
            1000.0 / self.fps
        } else {
            1000.0 / 60.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let stage = Stage {
            width: 1280,
            height: 720,
            fps: 30.0,
            bg_rgba: 0x1E1E_1EFF,
            root_character_id: 0,
        };
        assert_eq!(Stage::parse(&stage.to_bytes()).unwrap(), stage);
    }

    #[test]
    fn rejects_short_payload() {
        assert_eq!(
            Stage::parse(&[0u8; 8]),
            Err(Error::MalformedPayload("STAG"))
        );
    }

    #[test]
    fn zero_fps_falls_back_to_60() {
        let stage = Stage {
            width: 1,
            height: 1,
            fps: 0.0,
            bg_rgba: 0,
            root_character_id: 0,
        };
        assert!((stage.frame_duration_ms() - 1000.0 / 60.0).abs() < f32::EPSILON);
    }
}
