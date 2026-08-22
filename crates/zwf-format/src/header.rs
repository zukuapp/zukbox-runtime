//! 파일 헤더 — 명세 §3.

use crate::crc32;
use crate::error::{Error, Result};

/// `"ZWF1"`.
pub const MAGIC: [u8; 4] = *b"ZWF1";

/// 현재 헤더 크기. 명세가 필드를 추가하면 늘어난다.
pub const HEADER_SIZE: u32 = 32;

/// 이 크레이트가 읽을 수 있는 최대 `version_major`.
pub const SUPPORTED_VERSION_MAJOR: u8 = 0;

/// 파일 플래그 — 명세 §3.
pub mod flags {
    /// `SIGN` 청크가 마지막에 존재한다.
    pub const SIGNED: u16 = 1 << 0;
    /// 청크가 의존 순서로 정렬되어 스트리밍 파싱이 가능하다.
    pub const STREAMABLE: u16 = 1 << 1;
    /// 비트맵이 `ATLS` 텍스처 아틀라스로 병합되었다.
    pub const ATLAS_PACKED: u16 = 1 << 2;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub version_major: u8,
    pub version_minor: u8,
    pub flags: u16,
    pub header_size: u32,
    pub chunk_count: u32,
    pub file_size: u64,
}

impl Header {
    /// `bytes` 앞부분에서 헤더를 읽고 전부 검증한다.
    ///
    /// 검증 순서는 명세 §3을 따른다: 매직 → CRC → 버전 → 길이.
    /// CRC 를 버전보다 먼저 보는 이유는, 손상된 바이트가 만들어낸
    /// 엉뚱한 버전 번호로 "미지원 버전" 오진을 하지 않기 위해서다.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < HEADER_SIZE as usize {
            return Err(Error::TooShort);
        }

        if bytes[0..4] != MAGIC {
            return Err(Error::BadMagic);
        }

        let stored_crc = u32::from_le_bytes([bytes[28], bytes[29], bytes[30], bytes[31]]);
        if crc32::checksum(&bytes[0..28]) != stored_crc {
            return Err(Error::HeaderCrcMismatch);
        }

        let version_major = bytes[4];
        let version_minor = bytes[5];
        if version_major > SUPPORTED_VERSION_MAJOR {
            return Err(Error::UnsupportedVersion {
                major: version_major,
                minor: version_minor,
            });
        }

        let header_size = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        if header_size != HEADER_SIZE {
            return Err(Error::BadHeaderSize(header_size));
        }

        let file_size = u64::from_le_bytes([
            bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23],
        ]);
        if file_size != bytes.len() as u64 {
            return Err(Error::LengthMismatch {
                declared: file_size,
                actual: bytes.len() as u64,
            });
        }

        Ok(Header {
            version_major,
            version_minor,
            flags: u16::from_le_bytes([bytes[6], bytes[7]]),
            header_size,
            chunk_count: u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
            file_size,
        })
    }

    /// 헤더를 32바이트로 직렬화한다. CRC 는 여기서 계산해 채운다.
    pub fn to_bytes(&self) -> [u8; HEADER_SIZE as usize] {
        let mut out = [0u8; HEADER_SIZE as usize];
        out[0..4].copy_from_slice(&MAGIC);
        out[4] = self.version_major;
        out[5] = self.version_minor;
        out[6..8].copy_from_slice(&self.flags.to_le_bytes());
        out[8..12].copy_from_slice(&self.header_size.to_le_bytes());
        out[12..16].copy_from_slice(&self.chunk_count.to_le_bytes());
        out[16..24].copy_from_slice(&self.file_size.to_le_bytes());
        // 24..28 은 reserved, 0 유지.
        let crc = crc32::checksum(&out[0..28]);
        out[28..32].copy_from_slice(&crc.to_le_bytes());
        out
    }

    pub const fn is_signed(&self) -> bool {
        self.flags & flags::SIGNED != 0
    }

    pub const fn is_streamable(&self) -> bool {
        self.flags & flags::STREAMABLE != 0
    }

    pub const fn is_atlas_packed(&self) -> bool {
        self.flags & flags::ATLAS_PACKED != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Header {
        Header {
            version_major: 0,
            version_minor: 1,
            flags: flags::STREAMABLE,
            header_size: HEADER_SIZE,
            chunk_count: 4,
            file_size: HEADER_SIZE as u64,
        }
    }

    #[test]
    fn round_trip() {
        let header = sample();
        let bytes = header.to_bytes();
        assert_eq!(Header::parse(&bytes).unwrap(), header);
    }

    #[test]
    fn rejects_short_input() {
        assert_eq!(Header::parse(&[0u8; 8]), Err(Error::TooShort));
    }

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = sample().to_bytes();
        bytes[0] = b'X';
        assert_eq!(Header::parse(&bytes), Err(Error::BadMagic));
    }

    #[test]
    fn rejects_corrupted_byte() {
        let mut bytes = sample().to_bytes();
        bytes[12] ^= 0xFF; // chunk_count 손상
        assert_eq!(Header::parse(&bytes), Err(Error::HeaderCrcMismatch));
    }

    #[test]
    fn rejects_future_major_version() {
        let mut header = sample();
        header.version_major = SUPPORTED_VERSION_MAJOR + 1;
        let bytes = header.to_bytes();
        assert!(matches!(
            Header::parse(&bytes),
            Err(Error::UnsupportedVersion { .. })
        ));
    }

    #[test]
    fn rejects_length_mismatch() {
        let mut header = sample();
        header.file_size = 4096;
        let bytes = header.to_bytes();
        assert!(matches!(
            Header::parse(&bytes),
            Err(Error::LengthMismatch { .. })
        ));
    }
}
