//! 호스트(JS)와의 경계 — 순수 C ABI.
//!
//! wasm-bindgen 을 쓰지 않는다. 이유:
//!
//! 1. 생성 글루 없이 `WebAssembly.instantiateStreaming` 만으로 로드된다.
//! 2. 산출물이 작다 — `.zwf` 첫 재생 지연에 직결된다.
//! 3. 렌더 큐를 선형 메모리에 직접 쓰고 JS 가 `Float32Array` 뷰로 읽는,
//!    복사 없는 경로를 유지할 수 있다.
//!
//! # 규약
//!
//! - `i32` 반환값에서 **음수는 오류 코드**(`zwf_format::Error::code`)다.
//! - 실패 사유는 `zwf_last_error()` 로도 다시 읽을 수 있다.
//! - 문자열은 오가지 않는다. 오류는 숫자, FourCC 는 리틀엔디언 `u32`.

use crate::registry;
use crate::timeline::{self, FrameError, RENDER_ITEM_FLOATS};

/// 호스트가 파일 바이트를 써 넣을 버퍼를 확보한다.
///
/// # Safety
///
/// 반환된 포인터는 같은 `len` 으로 `zwf_free` 를 부르거나, `zwf_open` 에
/// 넘겨 소유권을 이전하기 전까지 유효하다.
#[unsafe(no_mangle)]
pub extern "C" fn zwf_alloc(len: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(len);
    let ptr = buffer.as_mut_ptr();
    core::mem::forget(buffer);
    ptr
}

/// `zwf_alloc` 이 준 버퍼를 반납한다.
///
/// # Safety
///
/// `ptr` 과 `len` 은 반드시 같은 `zwf_alloc` 호출에서 나온 짝이어야 한다.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn zwf_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: 호출자가 zwf_alloc 이 돌려준 (ptr, len) 짝임을 보장한다.
    unsafe {
        drop(Vec::from_raw_parts(ptr, 0, len));
    }
}

/// `.zwf` 를 파싱해 핸들을 돌려준다. 음수면 오류 코드.
///
/// 성공하면 버퍼의 소유권을 런타임이 가져가므로 호스트는 `zwf_free` 를
/// 부르면 안 된다. 실패하면 버퍼는 여기서 해제된다.
///
/// # Safety
///
/// `ptr` 은 `zwf_alloc(len)` 이 돌려준 포인터여야 한다.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn zwf_open(ptr: *mut u8, len: usize) -> i32 {
    if ptr.is_null() {
        registry::set_last_error(BAD_ARGUMENT);
        return BAD_ARGUMENT;
    }
    // SAFETY: 호출자가 zwf_alloc(len) 의 결과임을 보장한다. 용량과 길이를
    // 모두 len 으로 잡는 이유는 호스트가 버퍼를 가득 채워 넘기기 때문이다.
    let bytes = unsafe { Vec::from_raw_parts(ptr, len, len) };
    registry::open(bytes)
}

/// 핸들을 닫는다.
#[unsafe(no_mangle)]
pub extern "C" fn zwf_close(handle: i32) {
    registry::close(handle);
}

/// 마지막 실패의 오류 코드. 성공 직후에는 0.
#[unsafe(no_mangle)]
pub extern "C" fn zwf_last_error() -> i32 {
    registry::last_error()
}

/// 이 모듈이 구현하는 명세 버전. 상위 16비트 major, 하위 16비트 minor.
#[unsafe(no_mangle)]
pub extern "C" fn zwf_spec_version() -> u32 {
    let (major, minor) = zwf_format::SPEC_VERSION;
    ((major as u32) << 16) | minor as u32
}

/// 현재 열려 있는 인스턴스 수. 호스트 쪽 누수 테스트용.
#[unsafe(no_mangle)]
pub extern "C" fn zwf_open_count() -> u32 {
    registry::open_count() as u32
}

/// 인자가 잘못됐을 때의 코드. `zwf_format::Error` 코드 공간(-1..=-14)과 겹치지 않는다.
pub const BAD_ARGUMENT: i32 = -100;
/// 핸들이 유효하지 않을 때의 코드.
pub const BAD_HANDLE: i32 = -101;
/// 출력 버퍼가 부족할 때의 코드.
pub const BAD_BUFFER: i32 = -102;

macro_rules! stage_getter {
    ($name:ident, $ty:ty, $field:ident, $invalid:expr) => {
        /// 실패 시 `$invalid`. 사유는 `zwf_last_error()`.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(handle: i32) -> $ty {
            match registry::with(handle, |instance| instance.stage.$field) {
                Some(value) => {
                    registry::set_last_error(0);
                    value
                }
                None => {
                    registry::set_last_error(BAD_HANDLE);
                    $invalid
                }
            }
        }
    };
}

stage_getter!(zwf_stage_width, u32, width, 0);
stage_getter!(zwf_stage_height, u32, height, 0);
stage_getter!(zwf_stage_fps, f32, fps, 0.0);
stage_getter!(zwf_stage_bg_rgba, u32, bg_rgba, 0);
stage_getter!(zwf_root_character_id, u32, root_character_id, 0);

/// 파일에 들어 있는 청크 개수. 실패 시 음수.
#[unsafe(no_mangle)]
pub extern "C" fn zwf_chunk_count(handle: i32) -> i32 {
    match registry::with(handle, |instance| instance.chunk_ids.len() as i32) {
        Some(count) => {
            registry::set_last_error(0);
            count
        }
        None => {
            registry::set_last_error(BAD_HANDLE);
            BAD_HANDLE
        }
    }
}

/// `index` 번째 청크의 FourCC 를 리틀엔디언 `u32` 로 돌려준다. 없으면 0.
#[unsafe(no_mangle)]
pub extern "C" fn zwf_chunk_id_at(handle: i32, index: u32) -> u32 {
    let found = registry::with(handle, |instance| {
        instance
            .chunk_ids
            .get(index as usize)
            .map(|id| u32::from_le_bytes(id.0))
    });

    match found {
        Some(Some(id)) => {
            registry::set_last_error(0);
            id
        }
        Some(None) => {
            registry::set_last_error(BAD_ARGUMENT);
            0
        }
        None => {
            registry::set_last_error(BAD_HANDLE);
            0
        }
    }
}

/// 파일 플래그를 비트마스크로 돌려준다. bit0 SIGNED, bit1 STREAMABLE.
#[unsafe(no_mangle)]
pub extern "C" fn zwf_file_flags(handle: i32) -> i32 {
    let found = registry::with(handle, |instance| {
        (instance.is_signed as i32) | ((instance.is_streamable as i32) << 1)
    });

    match found {
        Some(flags) => {
            registry::set_last_error(0);
            flags
        }
        None => {
            registry::set_last_error(BAD_HANDLE);
            BAD_HANDLE
        }
    }
}

/// `CHRS` 테이블의 캐릭터 수. 실패 시 음수.
#[unsafe(no_mangle)]
pub extern "C" fn zwf_character_count(handle: i32) -> i32 {
    match registry::with(handle, |instance| instance.characters.entries.len() as i32) {
        Some(count) => {
            registry::set_last_error(0);
            count
        }
        None => {
            registry::set_last_error(BAD_HANDLE);
            BAD_HANDLE
        }
    }
}

/// `character_id` 의 kind (1=MovieClip … 5=Text). 없으면 0.
#[unsafe(no_mangle)]
pub extern "C" fn zwf_character_kind(handle: i32, character_id: u32) -> i32 {
    let found = registry::with(handle, |instance| {
        instance
            .characters
            .entries
            .get(character_id as usize)
            .map(|entry| entry.kind as i32)
    });

    match found {
        Some(Some(kind)) => {
            registry::set_last_error(0);
            kind
        }
        Some(None) => {
            registry::set_last_error(BAD_ARGUMENT);
            0
        }
        None => {
            registry::set_last_error(BAD_HANDLE);
            BAD_HANDLE
        }
    }
}

/// MovieClip 캐릭터의 `total_frame`. MovieClip 이 아니거나 없으면 -1.
#[unsafe(no_mangle)]
pub extern "C" fn zwf_mclip_total_frame(handle: i32, character_id: u32) -> i32 {
    let result = registry::with(handle, |instance| registry::movie_clip_total_frame(instance, character_id));
    match result {
        Some(Ok(total_frame)) => {
            registry::set_last_error(0);
            total_frame as i32
        }
        Some(Err(code)) => {
            registry::set_last_error(code);
            code
        }
        None => {
            registry::set_last_error(BAD_HANDLE);
            BAD_HANDLE
        }
    }
}

/// 렌더 목록 항목 하나가 차지하는 `f32` 개수 (현재 17).
#[unsafe(no_mangle)]
pub extern "C" fn zwf_render_item_stride() -> u32 {
    RENDER_ITEM_FLOATS as u32
}

/// MovieClip `character_id` 의 프레임 `frame` 을 평가한다.
///
/// - `out_ptr == null` 이거나 `out_capacity == 0` 이면 버퍼에 쓰지 않고 **항목
///   개수만** 돌려준다 (할당 크기 계산용).
/// - 그 외에는 `out_ptr` 에 `f32` 배열을 쓴다. `out_capacity` 는 **f32 개수**.
/// - 성공 시 항목 개수(>= 0). 실패 시 음수 오류 코드.
///
/// # Safety
///
/// `out_ptr` 이 null 이 아니면 `out_capacity` 개의 `f32` 를 쓸 수 있어야 한다.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn zwf_eval_frame(
    handle: i32,
    character_id: u32,
    frame: u32,
    out_ptr: *mut f32,
    out_capacity: u32,
) -> i32 {
    let result = registry::with(handle, |instance| {
        let body = registry::movie_clip_body(instance, character_id)?;
        timeline::evaluate_frame(&body, frame).map_err(|error| match error {
            FrameError::OutOfRange => BAD_ARGUMENT,
            FrameError::BufferTooSmall { .. } => BAD_BUFFER,
        })
    });

    match result {
        Some(Ok(items)) => {
            if out_ptr.is_null() || out_capacity == 0 {
                registry::set_last_error(0);
                return items.len() as i32;
            }

            // SAFETY: 호출자가 out_capacity 만큼의 f32 슬롯을 보장한다.
            let out = unsafe {
                core::slice::from_raw_parts_mut(out_ptr, out_capacity as usize)
            };

            match timeline::write_render_items(&items, out) {
                Ok(_) => {
                    registry::set_last_error(0);
                    items.len() as i32
                }
                Err(FrameError::BufferTooSmall { .. }) => {
                    registry::set_last_error(BAD_BUFFER);
                    BAD_BUFFER
                }
                Err(FrameError::OutOfRange) => {
                    registry::set_last_error(BAD_ARGUMENT);
                    BAD_ARGUMENT
                }
            }
        }
        Some(Err(code)) => {
            registry::set_last_error(code);
            code
        }
        None => {
            registry::set_last_error(BAD_HANDLE);
            BAD_HANDLE
        }
    }
}
