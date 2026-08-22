//! ZWF WebAssembly 런타임 코어.
//!
//! 이 크레이트가 하는 일과 하지 않는 일을 분명히 해 둔다.
//!
//! **한다**: `.zwf` 파싱·검증, 타임라인 평가, 행렬/컬러트랜스폼 합성,
//! 그 결과를 선형 메모리의 렌더 큐로 쓰기.
//!
//! **하지 않는다**: 픽셀을 그리지 않는다. 실제 래스터화는 Next2D Player 의
//! WebGL/WebGPU 렌더러(JS, 워커 스레드)가 맡는다. 스크립트도 실행하지 않는다
//! (명세 §5.9 보안 경계).
//!
//! ```text
//!   .zwf bytes
//!       │
//!       ▼
//!   zwf-runtime (wasm)  ── parse · timeline · transform
//!       │
//!       ▼  render queue (선형 메모리, 복사 없음)
//!   Next2D renderer worker ── WebGL2 / WebGPU
//!       │
//!       ▼
//!   <canvas>
//! ```
//!
//! 현재 상태: 파싱·스테이지 조회·타임라인 평가·렌더 목록 생성까지.

pub mod abi;
pub mod registry;
pub mod timeline;

pub use zwf_format;

#[cfg(test)]
mod tests {
    use crate::abi::{self, BAD_HANDLE};
    use crate::registry;
    use crate::timeline::RENDER_ITEM_FLOATS;
    use zwf_format::header::flags as file_flags;
    use zwf_format::{
        ArchiveWriter, CharacterEntry, CharacterKind, ChunkId, DictionaryEntry, MovieClipBody,
        PlaceObject, Stage,
    };

    fn sample_file() -> Vec<u8> {
        let stage = Stage {
            width: 1280,
            height: 720,
            fps: 24.0,
            bg_rgba: 0x1E1E_1EFF,
            root_character_id: 0,
        };

        let mut writer = ArchiveWriter::new(file_flags::STREAMABLE);
        writer.push_raw(ChunkId::META, br#"{"tool":"zukbox/0.1.0"}"#, 0);
        writer.push_raw(ChunkId::STAG, &stage.to_bytes(), 0);
        writer.push_raw(ChunkId::CHRS, &0u32.to_le_bytes(), 0);
        writer.push_raw(ChunkId::MCLP, &[], 0);
        writer.finish()
    }

    /// 호스트가 하는 일을 그대로 흉내낸다: alloc → 복사 → open.
    fn open_sample() -> i32 {
        let bytes = sample_file();
        let ptr = abi::zwf_alloc(bytes.len());
        // SAFETY: ptr 은 방금 bytes.len() 만큼 확보한 버퍼다.
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
            abi::zwf_open(ptr, bytes.len())
        }
    }

    #[test]
    fn opens_and_reads_stage() {
        let handle = open_sample();
        assert!(handle >= 0, "open failed with {handle}");

        assert_eq!(abi::zwf_stage_width(handle), 1280);
        assert_eq!(abi::zwf_stage_height(handle), 720);
        assert_eq!(abi::zwf_stage_fps(handle), 24.0);
        assert_eq!(abi::zwf_stage_bg_rgba(handle), 0x1E1E_1EFF);
        assert_eq!(abi::zwf_last_error(), 0);

        abi::zwf_close(handle);
    }

    #[test]
    fn reports_chunks() {
        let handle = open_sample();
        assert_eq!(abi::zwf_chunk_count(handle), 4);

        let first = abi::zwf_chunk_id_at(handle, 0).to_le_bytes();
        assert_eq!(&first, b"META");

        // 범위를 벗어난 인덱스는 0 과 오류 코드를 남긴다.
        assert_eq!(abi::zwf_chunk_id_at(handle, 99), 0);
        assert_eq!(abi::zwf_last_error(), abi::BAD_ARGUMENT);

        abi::zwf_close(handle);
    }

    #[test]
    fn reports_file_flags() {
        let handle = open_sample();
        assert_eq!(abi::zwf_file_flags(handle), 0b10); // STREAMABLE 만
        abi::zwf_close(handle);
    }

    #[test]
    fn rejects_garbage() {
        let garbage = [0xABu8; 64];
        let ptr = abi::zwf_alloc(garbage.len());
        // SAFETY: ptr 은 방금 확보한 같은 크기의 버퍼다.
        let handle = unsafe {
            core::ptr::copy_nonoverlapping(garbage.as_ptr(), ptr, garbage.len());
            abi::zwf_open(ptr, garbage.len())
        };

        assert!(handle < 0);
        assert_eq!(handle, abi::zwf_last_error());
    }

    #[test]
    fn invalid_handle_is_reported_not_panicked() {
        assert_eq!(abi::zwf_stage_width(9999), 0);
        assert_eq!(abi::zwf_last_error(), BAD_HANDLE);

        assert_eq!(abi::zwf_chunk_count(-5), BAD_HANDLE);
    }

    #[test]
    fn close_frees_the_slot_and_it_gets_reused() {
        let before = registry::open_count();

        let first = open_sample();
        assert_eq!(registry::open_count(), before + 1);

        abi::zwf_close(first);
        assert_eq!(registry::open_count(), before);

        // 비워진 슬롯을 다시 쓴다 — 핸들이 무한히 커지지 않는다.
        let second = open_sample();
        assert_eq!(second, first);
        abi::zwf_close(second);
    }

    #[test]
    fn spec_version_is_reported() {
        // major=0 이므로 상위 16비트는 0, 하위 16비트에 minor=1.
        assert_eq!(abi::zwf_spec_version(), 1);
    }

    fn timeline_file() -> Vec<u8> {
        let body = MovieClipBody {
            total_frame: 1,
            dictionary: vec![DictionaryEntry {
                character_id: 1,
                start_frame: 0,
                end_frame: 1,
                clip_depth: -1,
            }],
            depth_count: 1,
            place_objects: vec![PlaceObject {
                matrix: Some([1.0, 0.0, 0.0, 1.0, 100.0, 50.0]),
                ..Default::default()
            }],
            controller: vec![0],
            place_map: vec![0],
        };

        let characters = zwf_format::Characters {
            entries: vec![CharacterEntry {
                kind: CharacterKind::MovieClip,
                exported: true,
                body_offset: 0,
                body_size: body.to_bytes().len() as u32,
            }],
        };

        let stage = Stage {
            width: 640,
            height: 480,
            fps: 12.0,
            bg_rgba: 0x000000_ff,
            root_character_id: 0,
        };

        let mut writer = ArchiveWriter::new(file_flags::STREAMABLE);
        writer.push_raw(ChunkId::META, b"{}", 0);
        writer.push_raw(ChunkId::STAG, &stage.to_bytes(), 0);
        writer.push_raw(ChunkId::CHRS, &characters.to_bytes(), 0);
        writer.push_raw(ChunkId::MCLP, &body.to_bytes(), 0);
        writer.finish()
    }

    fn open_timeline() -> i32 {
        let bytes = timeline_file();
        let ptr = abi::zwf_alloc(bytes.len());
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
            abi::zwf_open(ptr, bytes.len())
        }
    }

    #[test]
    fn eval_frame_writes_render_list() {
        assert_eq!(abi::zwf_render_item_stride(), RENDER_ITEM_FLOATS as u32);

        let handle = open_timeline();
        assert!(handle >= 0);

        let count = unsafe { abi::zwf_eval_frame(handle, 0, 0, core::ptr::null_mut(), 0) };
        assert_eq!(count, 1);
        assert_eq!(abi::zwf_last_error(), 0);

        let mut out = [0.0f32; RENDER_ITEM_FLOATS];
        let written = unsafe {
            abi::zwf_eval_frame(
                handle,
                0,
                0,
                out.as_mut_ptr(),
                RENDER_ITEM_FLOATS as u32,
            )
        };
        assert_eq!(written, 1);
        assert_eq!(out[0], 0.0);
        assert_eq!(out[1], 1.0);
        assert_eq!(out[2], 1.0);
        assert_eq!(out[6], 100.0);
        assert_eq!(out[7], 50.0);

        abi::zwf_close(handle);
    }

    #[test]
    fn eval_frame_rejects_out_of_range() {
        let handle = open_timeline();
        let code = unsafe { abi::zwf_eval_frame(handle, 0, 9, core::ptr::null_mut(), 0) };
        assert_eq!(code, abi::BAD_ARGUMENT);
        abi::zwf_close(handle);
    }
}
