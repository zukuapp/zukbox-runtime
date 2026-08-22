//! 컨테이너 순회 — 명세 §4, §6.

extern crate alloc;

use alloc::vec::Vec;

use crate::chunk::{CHUNK_HEADER_SIZE, Chunk, ChunkHeader, ChunkId, align_up};
use crate::error::{Error, Result};
use crate::header::{HEADER_SIZE, Header};

/// 버퍼 전체를 훑지 않고 청크를 하나씩 꺼내는 반복자.
///
/// 스트리밍 파싱의 기반이다 — 아직 도착하지 않은 뒷부분을 건드리지 않는다.
pub struct ChunkReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
    remaining: u32,
    failed: bool,
}

impl<'a> ChunkReader<'a> {
    /// 헤더 **뒤**의 첫 청크부터 읽기 시작한다.
    pub fn new(bytes: &'a [u8], header: &Header) -> Self {
        ChunkReader {
            bytes,
            cursor: header.header_size as usize,
            remaining: header.chunk_count,
            failed: false,
        }
    }

    fn read_one(&mut self) -> Result<Chunk<'a>> {
        let start = self.cursor;
        let header_end = start
            .checked_add(CHUNK_HEADER_SIZE)
            .ok_or(Error::TruncatedChunk)?;
        if header_end > self.bytes.len() {
            return Err(Error::TruncatedChunk);
        }

        let raw = &self.bytes[start..header_end];
        let header = ChunkHeader {
            id: ChunkId([raw[0], raw[1], raw[2], raw[3]]),
            codec: raw[4],
            flags: raw[5],
            stored_size: u32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]),
            origin_size: u32::from_le_bytes([raw[12], raw[13], raw[14], raw[15]]),
        };

        let payload_end = header_end
            .checked_add(header.stored_size as usize)
            .ok_or(Error::TruncatedChunk)?;
        if payload_end > self.bytes.len() {
            return Err(Error::TruncatedChunk);
        }

        let crc_len = if header.has_crc() { 4 } else { 0 };
        let crc_end = payload_end
            .checked_add(crc_len)
            .ok_or(Error::TruncatedChunk)?;
        if crc_end > self.bytes.len() {
            return Err(Error::TruncatedChunk);
        }

        let crc = header.has_crc().then(|| {
            let slice = &self.bytes[payload_end..crc_end];
            u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]])
        });

        // 다음 청크는 4바이트 경계에서 시작한다.
        self.cursor = align_up(crc_end);

        Ok(Chunk {
            header,
            stored: &self.bytes[header_end..payload_end],
            offset: start,
            crc,
        })
    }
}

impl<'a> Iterator for ChunkReader<'a> {
    type Item = Result<Chunk<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed || self.remaining == 0 {
            return None;
        }

        match self.read_one() {
            Ok(chunk) => {
                self.remaining -= 1;
                Some(Ok(chunk))
            }
            Err(error) => {
                // 한 번 어긋나면 이후 오프셋은 전부 신뢰할 수 없다. 여기서 멈춘다.
                self.failed = true;
                Some(Err(error))
            }
        }
    }
}

/// 검증을 마친 `.zwf` 파일 전체.
///
/// 청크 페이로드는 여전히 원본 버퍼를 빌린 상태다 — `Archive` 는 색인일 뿐
/// 데이터 사본이 아니다.
pub struct Archive<'a> {
    pub header: Header,
    pub chunks: Vec<Chunk<'a>>,
    bytes: &'a [u8],
}

impl<'a> Archive<'a> {
    /// 파일을 통째로 파싱하고 구조 무결성까지 검사한다.
    ///
    /// 스트리밍이 필요하면 `Header::parse` + `ChunkReader` 를 직접 쓰면 된다.
    pub fn parse(bytes: &'a [u8]) -> Result<Self> {
        let header = Header::parse(bytes)?;

        let mut chunks = Vec::with_capacity(header.chunk_count as usize);
        for chunk in ChunkReader::new(bytes, &header) {
            chunks.push(chunk?);
        }

        if chunks.len() as u32 != header.chunk_count {
            return Err(Error::ChunkCountMismatch {
                declared: header.chunk_count,
                actual: chunks.len() as u32,
            });
        }

        let archive = Archive {
            header,
            chunks,
            bytes,
        };
        archive.check_required_chunks()?;
        Ok(archive)
    }

    fn check_required_chunks(&self) -> Result<()> {
        for (required, name) in ChunkId::REQUIRED {
            if self.find(required).is_none() {
                return Err(Error::MissingRequiredChunk(name));
            }
        }
        Ok(())
    }

    /// 주어진 ID 의 첫 청크.
    pub fn find(&self, id: ChunkId) -> Option<&Chunk<'a>> {
        self.chunks.iter().find(|chunk| chunk.id() == id)
    }

    /// `SIGN` 이 덮어야 하는 바이트 범위 — 명세 §5.10.
    ///
    /// 파일 시작부터 `SIGN` 청크 헤더 직전까지다. 서명이 없으면 `None`.
    pub fn signed_range(&self) -> Option<&'a [u8]> {
        let sign = self.find(ChunkId::SIGN)?;
        Some(&self.bytes[..sign.offset])
    }

    /// 전체 바이트. 서명 검증·해시 계산용.
    pub const fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

/// 테스트와 에디터 측 인코더가 공유하는 최소 라이터.
///
/// 압축은 하지 않는다 — `.zwf` 를 실제로 만드는 쪽은 에디터(TypeScript)이고,
/// 여기 라이터는 파서를 검증할 파일을 만들기 위한 것이다.
pub struct ArchiveWriter {
    body: Vec<u8>,
    chunk_count: u32,
    flags: u16,
}

impl ArchiveWriter {
    pub const fn new(flags: u16) -> Self {
        ArchiveWriter {
            body: Vec::new(),
            chunk_count: 0,
            flags,
        }
    }

    /// 무압축 청크를 추가한다.
    pub fn push_raw(&mut self, id: ChunkId, payload: &[u8], chunk_flags: u8) {
        let header = ChunkHeader {
            id,
            codec: crate::chunk::Codec::Raw as u8,
            flags: chunk_flags,
            stored_size: payload.len() as u32,
            origin_size: payload.len() as u32,
        };

        self.body.extend_from_slice(&header.to_bytes());
        self.body.extend_from_slice(payload);

        if header.has_crc() {
            self.body
                .extend_from_slice(&crate::crc32::checksum(payload).to_le_bytes());
        }

        // 다음 청크가 4바이트 경계에서 시작하도록 패딩.
        let padding = align_up(self.body.len()) - self.body.len();
        self.body.resize(self.body.len() + padding, 0);

        self.chunk_count += 1;
    }

    pub fn finish(self) -> Vec<u8> {
        let header = Header {
            version_major: 0,
            version_minor: 1,
            flags: self.flags,
            header_size: HEADER_SIZE,
            chunk_count: self.chunk_count,
            file_size: HEADER_SIZE as u64 + self.body.len() as u64,
        };

        let mut out = Vec::with_capacity(header.file_size as usize);
        out.extend_from_slice(&header.to_bytes());
        out.extend_from_slice(&self.body);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::flags as chunk_flags;
    use crate::header::flags as file_flags;

    /// 필수 청크를 모두 갖춘 최소 파일.
    fn minimal_file() -> Vec<u8> {
        let mut writer = ArchiveWriter::new(file_flags::STREAMABLE);
        writer.push_raw(ChunkId::META, br#"{"tool":"zukbox/0.1.0"}"#, 0);
        writer.push_raw(ChunkId::STAG, &[0u8; 24], 0);
        writer.push_raw(ChunkId::CHRS, &0u32.to_le_bytes(), 0);
        writer.push_raw(ChunkId::MCLP, &[], 0);
        writer.finish()
    }

    #[test]
    fn parses_minimal_file() {
        let bytes = minimal_file();
        let archive = Archive::parse(&bytes).unwrap();

        assert_eq!(archive.header.chunk_count, 4);
        assert!(archive.header.is_streamable());
        assert_eq!(archive.chunks.len(), 4);
        assert_eq!(
            archive
                .find(ChunkId::META)
                .unwrap()
                .payload()
                .unwrap()
                .as_ref(),
            br#"{"tool":"zukbox/0.1.0"}"#
        );
    }

    #[test]
    fn every_chunk_starts_aligned() {
        let bytes = minimal_file();
        let archive = Archive::parse(&bytes).unwrap();
        for chunk in &archive.chunks {
            assert_eq!(chunk.offset % 4, 0, "{:?} is misaligned", chunk.id());
        }
    }

    #[test]
    fn verifies_chunk_crc() {
        let mut writer = ArchiveWriter::new(0);
        writer.push_raw(ChunkId::META, b"{}", chunk_flags::HAS_CRC);
        writer.push_raw(ChunkId::STAG, &[0u8; 24], 0);
        writer.push_raw(ChunkId::CHRS, &0u32.to_le_bytes(), 0);
        writer.push_raw(ChunkId::MCLP, &[], 0);
        let bytes = writer.finish();

        let archive = Archive::parse(&bytes).unwrap();
        assert!(archive.find(ChunkId::META).unwrap().payload().is_ok());
    }

    #[test]
    fn rejects_missing_required_chunk() {
        let mut writer = ArchiveWriter::new(0);
        writer.push_raw(ChunkId::META, b"{}", 0);
        writer.push_raw(ChunkId::STAG, &[0u8; 24], 0);
        let bytes = writer.finish();

        assert!(matches!(
            Archive::parse(&bytes),
            Err(Error::MissingRequiredChunk(_))
        ));
    }

    #[test]
    fn rejects_truncated_chunk() {
        let mut bytes = minimal_file();
        // 마지막 청크의 페이로드를 잘라낸다. file_size 도 맞춰 헤더 CRC 를 다시 쓴다.
        bytes.truncate(bytes.len() - 8);
        let mut header = Header::parse(&minimal_file()).unwrap();
        header.file_size = bytes.len() as u64;
        bytes[..HEADER_SIZE as usize].copy_from_slice(&header.to_bytes());

        assert!(Archive::parse(&bytes).is_err());
    }

    #[test]
    fn parses_asset_chunks() {
        use crate::bmap::{BitmapBody, BitmapEncoding};
        use crate::chrs::{CharacterEntry, CharacterKind, Characters};
        use crate::shap::ShapeBody;
        use crate::vids::{VideoBody, VideoEncoding};

        let shape = ShapeBody {
            bounds: [0.0, 100.0, 0.0, 50.0],
            in_bitmap: false,
            grid: None,
            bitmap_id: None,
            recodes: vec![1.0, 2.0, 3.0],
        };
        let bitmap = BitmapBody {
            bounds: [0.0, 2.0, 0.0, 2.0],
            encoding: BitmapEncoding::RawRgba8,
            data: vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255],
        };
        let video = VideoBody {
            bounds: [0.0, 320.0, 0.0, 240.0],
            volume: 0.8,
            loop_playback: true,
            auto_play: false,
            encoding: VideoEncoding::Mp4,
            data: vec![0, 0, 0, 0x20, 0x66, 0x74, 0x79, 0x70],
        };

        let shap_payload = shape.to_bytes();
        let bmap_payload = bitmap.to_bytes();
        let vids_payload = video.to_bytes();
        let characters = Characters {
            entries: vec![
                CharacterEntry {
                    kind: CharacterKind::Shape,
                    exported: false,
                    body_offset: 0,
                    body_size: shap_payload.len() as u32,
                },
                CharacterEntry {
                    kind: CharacterKind::Bitmap,
                    exported: false,
                    body_offset: 0,
                    body_size: bmap_payload.len() as u32,
                },
                CharacterEntry {
                    kind: CharacterKind::Video,
                    exported: false,
                    body_offset: 0,
                    body_size: vids_payload.len() as u32,
                },
            ],
        };

        let mut writer = ArchiveWriter::new(file_flags::STREAMABLE);
        writer.push_raw(ChunkId::META, b"{}", 0);
        writer.push_raw(ChunkId::STAG, &[0u8; 24], 0);
        writer.push_raw(ChunkId::CHRS, &characters.to_bytes(), 0);
        writer.push_raw(ChunkId::SHAP, &shap_payload, 0);
        writer.push_raw(ChunkId::BMAP, &bmap_payload, 0);
        writer.push_raw(ChunkId::MCLP, &[], 0);
        writer.push_raw(ChunkId::VIDS, &vids_payload, 0);
        let bytes = writer.finish();

        let archive = Archive::parse(&bytes).unwrap();
        let parsed_characters =
            Characters::parse(archive.find(ChunkId::CHRS).unwrap().payload().unwrap().as_ref())
                .unwrap();
        let shap_chunk = archive.find(ChunkId::SHAP).unwrap().payload().unwrap();
        let bmap_chunk = archive.find(ChunkId::BMAP).unwrap().payload().unwrap();
        let vids_chunk = archive.find(ChunkId::VIDS).unwrap().payload().unwrap();

        assert_eq!(parsed_characters.entries.len(), 3);
        assert_eq!(
            ShapeBody::parse_at(
                shap_chunk.as_ref(),
                parsed_characters.entries[0].body_offset,
                parsed_characters.entries[0].body_size
            )
            .unwrap(),
            shape
        );
        assert_eq!(
            BitmapBody::parse_at(
                bmap_chunk.as_ref(),
                parsed_characters.entries[1].body_offset,
                parsed_characters.entries[1].body_size
            )
            .unwrap(),
            bitmap
        );
        assert_eq!(
            VideoBody::parse_at(
                vids_chunk.as_ref(),
                parsed_characters.entries[2].body_offset,
                parsed_characters.entries[2].body_size
            )
            .unwrap(),
            video
        );
    }

    #[test]
    fn signed_range_stops_before_signature() {
        let mut writer = ArchiveWriter::new(file_flags::SIGNED);
        writer.push_raw(ChunkId::META, b"{}", 0);
        writer.push_raw(ChunkId::STAG, &[0u8; 24], 0);
        writer.push_raw(ChunkId::CHRS, &0u32.to_le_bytes(), 0);
        writer.push_raw(ChunkId::MCLP, &[], 0);
        writer.push_raw(ChunkId::SIGN, &[0u8; 12], 0);
        let bytes = writer.finish();

        let archive = Archive::parse(&bytes).unwrap();
        let signed = archive.signed_range().unwrap();
        let sign_offset = archive.find(ChunkId::SIGN).unwrap().offset;

        assert_eq!(signed.len(), sign_offset);
        assert!(signed.len() < bytes.len());
    }
}
