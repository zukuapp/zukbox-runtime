//! `CHRS` 청크 — 명세 §5.4.

extern crate alloc;

use alloc::vec::Vec;

use crate::error::{Error, Result};

/// 캐릭터 종류 — `IPublishObject.characters[].extends` 와 대응.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CharacterKind {
    MovieClip = 1,
    Shape = 2,
    Bitmap = 3,
    Video = 4,
    Text = 5,
}

impl CharacterKind {
    pub const fn from_u8(value: u8) -> Option<CharacterKind> {
        match value {
            1 => Some(CharacterKind::MovieClip),
            2 => Some(CharacterKind::Shape),
            3 => Some(CharacterKind::Bitmap),
            4 => Some(CharacterKind::Video),
            5 => Some(CharacterKind::Text),
            _ => None,
        }
    }
}

/// `CHRS` 테이블의 한 행. **배열 인덱스 = `characterId`.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharacterEntry {
    pub kind: CharacterKind,
    /// bit0 = exported (SYMB 에 이름 있음).
    pub exported: bool,
    /// 본문 청크(MCLP/SHAP/BMAP/VIDS) 내 바이트 오프셋.
    pub body_offset: u32,
    /// 본문 바이트 수.
    pub body_size: u32,
}

/// `CHRS` 페이로드 전체.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Characters {
    pub entries: Vec<CharacterEntry>,
}

const ENTRY_SIZE: usize = 12;

impl Characters {
    pub fn parse(payload: &[u8]) -> Result<Self> {
        if payload.len() < 4 {
            return Err(Error::MalformedPayload("CHRS"));
        }

        let count = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
        let expected = 4 + count * ENTRY_SIZE;
        if payload.len() < expected {
            return Err(Error::MalformedPayload("CHRS"));
        }

        let mut entries = Vec::with_capacity(count);
        for idx in 0..count {
            let base = 4 + idx * ENTRY_SIZE;
            let kind = CharacterKind::from_u8(payload[base])
                .ok_or(Error::MalformedPayload("CHRS"))?;
            let flags = payload[base + 1];
            entries.push(CharacterEntry {
                kind,
                exported: flags & 1 != 0,
                body_offset: u32::from_le_bytes([
                    payload[base + 4],
                    payload[base + 5],
                    payload[base + 6],
                    payload[base + 7],
                ]),
                body_size: u32::from_le_bytes([
                    payload[base + 8],
                    payload[base + 9],
                    payload[base + 10],
                    payload[base + 11],
                ]),
            });
        }

        Ok(Characters { entries })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + self.entries.len() * ENTRY_SIZE);
        out.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        for entry in &self.entries {
            out.push(entry.kind as u8);
            out.push(if entry.exported { 1 } else { 0 });
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&entry.body_offset.to_le_bytes());
            out.extend_from_slice(&entry.body_size.to_le_bytes());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let characters = Characters {
            entries: vec![
                CharacterEntry {
                    kind: CharacterKind::MovieClip,
                    exported: true,
                    body_offset: 0,
                    body_size: 48,
                },
                CharacterEntry {
                    kind: CharacterKind::Bitmap,
                    exported: false,
                    body_offset: 0,
                    body_size: 0,
                },
            ],
        };
        assert_eq!(Characters::parse(&characters.to_bytes()).unwrap(), characters);
    }

    #[test]
    fn rejects_unknown_kind() {
        let mut bytes = vec![1, 0, 0, 0, 99, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(
            Characters::parse(&bytes),
            Err(Error::MalformedPayload("CHRS"))
        );
        bytes[4] = 1;
        assert!(Characters::parse(&bytes).is_ok());
    }
}
