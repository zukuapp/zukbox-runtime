//! 청크 TLV — 명세 §4.

extern crate alloc;

use alloc::borrow::Cow;
use alloc::vec::Vec;
use core::fmt;

use crate::crc32;
use crate::error::{Error, Result};

/// 청크 헤더 크기.
pub const CHUNK_HEADER_SIZE: usize = 16;

/// 모든 청크 경계의 정렬 단위.
pub const ALIGNMENT: usize = 4;

/// 4바이트 ASCII 식별자. 4자 미만은 공백으로 우측 패딩한다.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChunkId(pub [u8; 4]);

impl ChunkId {
    pub const META: ChunkId = ChunkId(*b"META");
    pub const STAG: ChunkId = ChunkId(*b"STAG");
    pub const SYMB: ChunkId = ChunkId(*b"SYMB");
    pub const CHRS: ChunkId = ChunkId(*b"CHRS");
    pub const MCLP: ChunkId = ChunkId(*b"MCLP");
    pub const SHAP: ChunkId = ChunkId(*b"SHAP");
    pub const BMAP: ChunkId = ChunkId(*b"BMAP");
    pub const ATLS: ChunkId = ChunkId(*b"ATLS");
    pub const SNDS: ChunkId = ChunkId(*b"SNDS");
    pub const VIDS: ChunkId = ChunkId(*b"VIDS");
    pub const SCPT: ChunkId = ChunkId(*b"SCPT");
    pub const SIGN: ChunkId = ChunkId(*b"SIGN");

    /// 재생에 반드시 필요한 청크. 하나라도 없으면 파일을 거부한다.
    ///
    /// 이름을 함께 들고 다니는 이유는 `Error::MissingRequiredChunk` 가
    /// `&'static str` 을 요구하기 때문이다 — `as_str` 은 `self` 를 빌린다.
    pub const REQUIRED: [(ChunkId, &'static str); 4] = [
        (Self::META, "META"),
        (Self::STAG, "STAG"),
        (Self::CHRS, "CHRS"),
        (Self::MCLP, "MCLP"),
    ];

    pub const fn as_str(&self) -> &str {
        // FourCC 는 ASCII 로만 쓰기로 명세에 고정되어 있다.
        match core::str::from_utf8(&self.0) {
            Ok(s) => s,
            Err(_) => "????",
        }
    }
}

impl fmt::Debug for ChunkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChunkId({})", self.as_str())
    }
}

/// 페이로드 압축 방식 — 명세 §4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Codec {
    /// 무압축.
    Raw = 0,
    /// RFC 1951 raw DEFLATE (zlib 래퍼 없음).
    Deflate = 1,
}

impl Codec {
    pub const fn from_u8(value: u8) -> Option<Codec> {
        match value {
            0 => Some(Codec::Raw),
            1 => Some(Codec::Deflate),
            _ => None,
        }
    }
}

/// 청크 플래그 — 명세 §4.
pub mod flags {
    /// 이 청크를 모르는 런타임은 건너뛰어도 된다.
    pub const SKIPPABLE: u8 = 1 << 0;
    /// 페이로드 뒤에 원본 기준 CRC-32 가 붙어 있다.
    pub const HAS_CRC: u8 = 1 << 1;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkHeader {
    pub id: ChunkId,
    pub codec: u8,
    pub flags: u8,
    pub stored_size: u32,
    pub origin_size: u32,
}

impl ChunkHeader {
    pub const fn has_crc(&self) -> bool {
        self.flags & flags::HAS_CRC != 0
    }

    pub const fn is_skippable(&self) -> bool {
        self.flags & flags::SKIPPABLE != 0
    }

    pub fn to_bytes(&self) -> [u8; CHUNK_HEADER_SIZE] {
        let mut out = [0u8; CHUNK_HEADER_SIZE];
        out[0..4].copy_from_slice(&self.id.0);
        out[4] = self.codec;
        out[5] = self.flags;
        // 6..8 reserved
        out[8..12].copy_from_slice(&self.stored_size.to_le_bytes());
        out[12..16].copy_from_slice(&self.origin_size.to_le_bytes());
        out
    }
}

/// 파일 안에 자리잡은 청크 하나. 페이로드는 복사하지 않고 원본을 빌린다.
#[derive(Debug, Clone, Copy)]
pub struct Chunk<'a> {
    pub header: ChunkHeader,
    /// 디스크에 있는 그대로의 바이트 (압축되어 있을 수 있다).
    pub stored: &'a [u8],
    /// 파일 시작점 기준 이 청크 헤더의 오프셋. 진단·서명 범위 계산용.
    pub offset: usize,
    /// `HAS_CRC` 일 때 페이로드 뒤에 붙어 있던 CRC-32. `ChunkReader` 가 채운다.
    pub(crate) crc: Option<u32>,
}

impl<'a> Chunk<'a> {
    pub const fn id(&self) -> ChunkId {
        self.header.id
    }

    /// 사용 가능한 페이로드를 돌려준다.
    ///
    /// `Raw` 는 원본을 그대로 빌려주고(제로카피), 압축된 청크만 할당한다.
    /// `HAS_CRC` 가 있으면 압축 해제 **후** 원본 바이트에 대해 검증한다.
    pub fn payload(&self) -> Result<Cow<'a, [u8]>> {
        let codec =
            Codec::from_u8(self.header.codec).ok_or(Error::UnknownCodec(self.header.codec))?;

        let data: Cow<'a, [u8]> = match codec {
            Codec::Raw => {
                if self.header.origin_size != self.header.stored_size {
                    return Err(Error::SizeMismatch);
                }
                Cow::Borrowed(self.stored)
            }
            Codec::Deflate => Cow::Owned(inflate(self.stored, self.header.origin_size as usize)?),
        };

        if self.header.has_crc() {
            let expected = self.trailing_crc().ok_or(Error::TruncatedChunk)?;
            if crc32::checksum(&data) != expected {
                return Err(Error::DecompressFailed);
            }
        }

        Ok(data)
    }

    /// `HAS_CRC` 일 때 페이로드 뒤에 붙어 있던 4바이트.
    ///
    /// `stored` 슬라이스는 페이로드까지만 가리키므로 CRC 는 원본 버퍼에서
    /// 따로 잘라와야 하고, 그 일은 `ChunkReader` 가 미리 해둔다.
    const fn trailing_crc(&self) -> Option<u32> {
        self.crc
    }
}

/// 4의 배수로 올림. 명세 §2 정렬 규칙.
pub const fn align_up(value: usize) -> usize {
    value.div_ceil(ALIGNMENT) * ALIGNMENT
}

#[cfg(feature = "deflate")]
fn inflate(input: &[u8], origin_size: usize) -> Result<Vec<u8>> {
    miniz_oxide::inflate::decompress_to_vec_with_limit(input, origin_size)
        .map_err(|_| Error::DecompressFailed)
}

#[cfg(not(feature = "deflate"))]
fn inflate(_input: &[u8], _origin_size: usize) -> Result<Vec<u8>> {
    Err(Error::CodecUnavailable(Codec::Deflate as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alignment_rounds_up() {
        assert_eq!(align_up(0), 0);
        assert_eq!(align_up(1), 4);
        assert_eq!(align_up(4), 4);
        assert_eq!(align_up(5), 8);
    }

    #[test]
    fn fourcc_prints_readably() {
        assert_eq!(ChunkId::MCLP.as_str(), "MCLP");
        assert_eq!(ChunkId(*b"SND ").as_str(), "SND ");
    }

    #[test]
    fn codec_rejects_unknown() {
        assert_eq!(Codec::from_u8(0), Some(Codec::Raw));
        assert_eq!(Codec::from_u8(1), Some(Codec::Deflate));
        assert_eq!(Codec::from_u8(9), None);
    }
}
