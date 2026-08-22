//! 파싱 실패 사유.
//!
//! 모든 변형은 ABI 경계를 넘어야 하므로 안정된 `i32` 코드를 갖는다.
//! 값이 정해지면 절대 바꾸지 않는다 — JS 로더가 숫자로 분기한다.

use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// 파일이 32바이트 헤더보다 짧다.
    TooShort,
    /// 매직 넘버가 `ZWF1` 이 아니다.
    BadMagic,
    /// 헤더 CRC 불일치 — 전송 중 손상.
    HeaderCrcMismatch,
    /// `version_major` 가 이 런타임의 지원 범위를 넘는다.
    UnsupportedVersion { major: u8, minor: u8 },
    /// `header_size` 가 알려진 값이 아니다.
    BadHeaderSize(u32),
    /// `file_size` 가 실제 버퍼 길이와 다르다.
    LengthMismatch { declared: u64, actual: u64 },
    /// 청크 헤더가 버퍼 끝을 넘어간다.
    TruncatedChunk,
    /// `chunk_count` 와 실제로 읽힌 청크 수가 다르다.
    ChunkCountMismatch { declared: u32, actual: u32 },
    /// 알 수 없는 `codec` 값이면서 `SKIPPABLE` 도 아니다.
    UnknownCodec(u8),
    /// `codec=RAW` 인데 `origin_size != stored_size`.
    SizeMismatch,
    /// 압축 해제 실패.
    DecompressFailed,
    /// 이 빌드에 압축 해제 기능이 없다(`deflate` feature 꺼짐).
    CodecUnavailable(u8),
    /// 필수 청크(`META`/`STAG`/`CHRS`/`MCLP`)가 없다.
    MissingRequiredChunk(&'static str),
    /// 청크 페이로드가 자기 스키마상 필요한 길이에 못 미친다.
    MalformedPayload(&'static str),
}

impl Error {
    /// FFI 로 넘길 안정 코드. 음수만 쓴다(0 이상은 성공값 영역).
    pub const fn code(&self) -> i32 {
        match self {
            Error::TooShort => -1,
            Error::BadMagic => -2,
            Error::HeaderCrcMismatch => -3,
            Error::UnsupportedVersion { .. } => -4,
            Error::BadHeaderSize(_) => -5,
            Error::LengthMismatch { .. } => -6,
            Error::TruncatedChunk => -7,
            Error::ChunkCountMismatch { .. } => -8,
            Error::UnknownCodec(_) => -9,
            Error::SizeMismatch => -10,
            Error::DecompressFailed => -11,
            Error::CodecUnavailable(_) => -12,
            Error::MissingRequiredChunk(_) => -13,
            Error::MalformedPayload(_) => -14,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::TooShort => f.write_str("file shorter than the 32-byte ZWF header"),
            Error::BadMagic => f.write_str("magic is not \"ZWF1\""),
            Error::HeaderCrcMismatch => f.write_str("header CRC-32 mismatch"),
            Error::UnsupportedVersion { major, minor } => {
                write!(f, "unsupported format version {major}.{minor}")
            }
            Error::BadHeaderSize(size) => write!(f, "unexpected header_size {size}"),
            Error::LengthMismatch { declared, actual } => {
                write!(f, "file_size {declared} but buffer is {actual}")
            }
            Error::TruncatedChunk => f.write_str("chunk extends past end of buffer"),
            Error::ChunkCountMismatch { declared, actual } => {
                write!(f, "chunk_count {declared} but found {actual}")
            }
            Error::UnknownCodec(codec) => write!(f, "unknown codec {codec}"),
            Error::SizeMismatch => f.write_str("RAW chunk with origin_size != stored_size"),
            Error::DecompressFailed => f.write_str("decompression failed"),
            Error::CodecUnavailable(codec) => {
                write!(f, "codec {codec} not compiled into this build")
            }
            Error::MissingRequiredChunk(id) => write!(f, "missing required chunk {id}"),
            Error::MalformedPayload(id) => write!(f, "malformed payload in chunk {id}"),
        }
    }
}

pub type Result<T> = core::result::Result<T, Error>;
