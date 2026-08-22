//! ZWF 컨테이너 포맷 — 파싱과 검증.
//!
//! `docs/zwf-format-v0.md` 명세의 구현체다. 명세와 이 크레이트가 어긋나면
//! **명세가 맞다** — 코드를 고친다.
//!
//! `no_std` 다. 브라우저 WASM 과 네이티브 도구(에디터 검증기, CLI)에서
//! 같은 코드를 쓰기 위해서이며, 힙은 `alloc` 만 요구한다.
//!
//! # 예시
//!
//! ```no_run
//! use zwf_format::{Archive, ChunkId};
//!
//! # fn main() -> Result<(), zwf_format::Error> {
//! # let bytes: &[u8] = &[];
//! let archive = Archive::parse(bytes)?;
//! let stage = archive.find(ChunkId::STAG).expect("STAG is required");
//! let payload = stage.payload()?;
//! # Ok(())
//! # }
//! ```

// 테스트 하네스는 std 를 요구한다. 라이브러리 자체는 no_std 로 남는다.
#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod bmap;
pub mod chrs;
pub mod chunk;
pub mod crc32;
pub mod error;
pub mod header;
pub mod mclip;
pub mod reader;
pub mod shap;
pub mod stage;
pub mod vids;

pub use bmap::{BitmapBody, BitmapEncoding};
pub use chrs::{CharacterEntry, CharacterKind, Characters};
pub use chunk::{Chunk, ChunkHeader, ChunkId, Codec};
pub use error::{Error, Result};
pub use header::Header;
pub use mclip::{DictionaryEntry, MovieClipBody, PlaceObject};
pub use reader::{Archive, ArchiveWriter, ChunkReader};
pub use shap::ShapeBody;
pub use stage::Stage;
pub use vids::{VideoBody, VideoEncoding};

/// 이 크레이트가 구현하는 명세 버전.
pub const SPEC_VERSION: (u8, u8) = (0, 1);
