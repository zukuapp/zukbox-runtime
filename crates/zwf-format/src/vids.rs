//! `VIDS` 청크 — 명세 §5.8.
//!
//! 명세 본문은 재생 파라미터를 헤더에 둔다고만 적고 바이너리 레이아웃은 `BMAP` 과
//! 같은 패턴으로 고정한다:
//!
//! ```text
//! f32 x_min, x_max, y_min, y_max
//! f32 volume
//! u8  flags         // bit0=loop, bit1=auto_play
//! u8  encoding      // 0=MP4, 1=WEBM, 2=OGG
//! u8  pad[2]
//! u32 byte_len
//! u8[byte_len] data
//! ```

extern crate alloc;

use alloc::vec::Vec;

use crate::error::{Error, Result};

pub const FLAG_LOOP: u8 = 1 << 0;
pub const FLAG_AUTO_PLAY: u8 = 1 << 1;

/// 비디오 컨테이너 인코딩.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VideoEncoding {
    Mp4 = 0,
    Webm = 1,
    Ogg = 2,
}

impl VideoEncoding {
    pub const fn from_u8(value: u8) -> Option<VideoEncoding> {
        match value {
            0 => Some(VideoEncoding::Mp4),
            1 => Some(VideoEncoding::Webm),
            2 => Some(VideoEncoding::Ogg),
            _ => None,
        }
    }
}

/// 비디오 본문 — `IVideoPublishJson`.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoBody {
    pub bounds: [f32; 4],
    pub volume: f32,
    pub loop_playback: bool,
    pub auto_play: bool,
    pub encoding: VideoEncoding,
    pub data: Vec<u8>,
}

const HEADER_SIZE: usize = 28;

impl VideoBody {
    pub fn parse(payload: &[u8]) -> Result<Self> {
        if payload.len() < HEADER_SIZE {
            return Err(Error::MalformedPayload("VIDS"));
        }

        let bounds = [
            f32::from_bits(u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]])),
            f32::from_bits(u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]])),
            f32::from_bits(u32::from_le_bytes([payload[8], payload[9], payload[10], payload[11]])),
            f32::from_bits(u32::from_le_bytes([
                payload[12], payload[13], payload[14], payload[15],
            ])),
        ];
        let volume = f32::from_bits(u32::from_le_bytes([
            payload[16], payload[17], payload[18], payload[19],
        ]));
        let flags = payload[20];
        let encoding = VideoEncoding::from_u8(payload[21]).ok_or(Error::MalformedPayload("VIDS"))?;
        let byte_len = u32::from_le_bytes([payload[24], payload[25], payload[26], payload[27]]) as usize;
        let end = HEADER_SIZE
            .checked_add(byte_len)
            .ok_or(Error::MalformedPayload("VIDS"))?;
        if end > payload.len() {
            return Err(Error::MalformedPayload("VIDS"));
        }

        Ok(VideoBody {
            bounds,
            volume,
            loop_playback: flags & FLAG_LOOP != 0,
            auto_play: flags & FLAG_AUTO_PLAY != 0,
            encoding,
            data: payload[HEADER_SIZE..end].to_vec(),
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut flags = 0u8;
        if self.loop_playback {
            flags |= FLAG_LOOP;
        }
        if self.auto_play {
            flags |= FLAG_AUTO_PLAY;
        }

        let mut out = Vec::with_capacity(HEADER_SIZE + self.data.len());
        for value in &self.bounds {
            out.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        out.extend_from_slice(&self.volume.to_bits().to_le_bytes());
        out.push(flags);
        out.push(self.encoding as u8);
        out.extend_from_slice(&[0u8; 2]);
        out.extend_from_slice(&(self.data.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.data);
        out
    }

    /// `VIDS` 청크 전체에서 `body_offset` 위치의 본문을 파싱한다.
    pub fn parse_at(vids_payload: &[u8], body_offset: u32, body_size: u32) -> Result<Self> {
        let start = body_offset as usize;
        let end = start
            .checked_add(body_size as usize)
            .ok_or(Error::MalformedPayload("VIDS"))?;
        if end > vids_payload.len() {
            return Err(Error::MalformedPayload("VIDS"));
        }
        Self::parse(&vids_payload[start..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mp4_round_trip() {
        let body = VideoBody {
            bounds: [0.0, 320.0, 0.0, 240.0],
            volume: 0.75,
            loop_playback: true,
            auto_play: false,
            encoding: VideoEncoding::Mp4,
            data: vec![0x00, 0x00, 0x00, 0x20, 0x66, 0x74, 0x79, 0x70],
        };
        assert_eq!(VideoBody::parse(&body.to_bytes()).unwrap(), body);
    }

    #[test]
    fn webm_round_trip() {
        let body = VideoBody {
            bounds: [0.0, 1.0, 0.0, 1.0],
            volume: 1.0,
            loop_playback: false,
            auto_play: true,
            encoding: VideoEncoding::Webm,
            data: vec![0x1A, 0x45, 0xDF, 0xA3],
        };
        assert_eq!(VideoBody::parse(&body.to_bytes()).unwrap(), body);
    }

    #[test]
    fn parse_at_slice() {
        let body = VideoBody {
            bounds: [0.0, 1.0, 0.0, 1.0],
            volume: 0.5,
            loop_playback: false,
            auto_play: false,
            encoding: VideoEncoding::Ogg,
            data: vec![0x4F, 0x67, 0x67, 0x53],
        };
        let chunk = [0u8; 12]
            .into_iter()
            .chain(body.to_bytes())
            .collect::<Vec<_>>();
        assert_eq!(
            VideoBody::parse_at(&chunk, 12, body.to_bytes().len() as u32).unwrap(),
            body
        );
    }

    #[test]
    fn rejects_unknown_encoding() {
        let mut bytes = VideoBody {
            bounds: [0.0, 1.0, 0.0, 1.0],
            volume: 1.0,
            loop_playback: false,
            auto_play: false,
            encoding: VideoEncoding::Mp4,
            data: vec![],
        }
        .to_bytes();
        bytes[21] = 99;
        assert_eq!(
            VideoBody::parse(&bytes),
            Err(Error::MalformedPayload("VIDS"))
        );
    }
}
