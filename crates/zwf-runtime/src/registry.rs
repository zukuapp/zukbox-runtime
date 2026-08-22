//! 열린 파일들의 보관소.
//!
//! WASM 은 단일 스레드이므로 `thread_local!` 하나면 충분하고, 락이 필요 없다.

use core::cell::RefCell;

use zwf_format::{Archive, Characters, ChunkId, Error, MovieClipBody, Stage};

use crate::abi::BAD_ARGUMENT;

/// 파싱을 마친 파일 하나.
///
/// `Archive` 는 원본 바이트를 빌리는 자기참조 구조라 그대로 보관할 수 없다.
/// 그래서 열 때 필요한 것만 소유 형태로 꺼내 두고, 바이트는 따로 붙잡아
/// 나중에 청크를 다시 훑을 수 있게 한다.
pub struct Instance {
    pub bytes: Vec<u8>,
    pub stage: Stage,
    pub characters: Characters,
    pub chunk_ids: Vec<ChunkId>,
    pub is_signed: bool,
    pub is_streamable: bool,
}

impl Instance {
    fn open(bytes: Vec<u8>) -> Result<Self, Error> {
        let archive = Archive::parse(&bytes)?;

        let stage_chunk = archive
            .find(ChunkId::STAG)
            .ok_or(Error::MissingRequiredChunk("STAG"))?;
        let stage = Stage::parse(&stage_chunk.payload()?)?;

        let chrs_chunk = archive
            .find(ChunkId::CHRS)
            .ok_or(Error::MissingRequiredChunk("CHRS"))?;
        let characters = Characters::parse(&chrs_chunk.payload()?)?;

        let chunk_ids = archive.chunks.iter().map(|chunk| chunk.id()).collect();
        let is_signed = archive.header.is_signed();
        let is_streamable = archive.header.is_streamable();

        drop(archive);

        Ok(Instance {
            bytes,
            stage,
            characters,
            chunk_ids,
            is_signed,
            is_streamable,
        })
    }
}

thread_local! {
    /// 슬롯 배열. 닫힌 자리는 `None` 이 되고 다음 `open` 이 재사용한다.
    static INSTANCES: RefCell<Vec<Option<Instance>>> = const { RefCell::new(Vec::new()) };
    /// 마지막으로 실패한 호출의 오류 코드. 성공하면 0 으로 지워진다.
    static LAST_ERROR: RefCell<i32> = const { RefCell::new(0) };
}

/// 파일을 열고 핸들을 돌려준다. 실패하면 음수 오류 코드.
pub fn open(bytes: Vec<u8>) -> i32 {
    match Instance::open(bytes) {
        Ok(instance) => {
            set_last_error(0);
            INSTANCES.with_borrow_mut(|slots| {
                if let Some(index) = slots.iter().position(Option::is_none) {
                    slots[index] = Some(instance);
                    index as i32
                } else {
                    slots.push(Some(instance));
                    (slots.len() - 1) as i32
                }
            })
        }
        Err(error) => {
            let code = error.code();
            set_last_error(code);
            code
        }
    }
}

/// 핸들을 닫고 메모리를 돌려준다. 이미 닫혀 있으면 아무 일도 하지 않는다.
pub fn close(handle: i32) {
    if handle < 0 {
        return;
    }
    INSTANCES.with_borrow_mut(|slots| {
        if let Some(slot) = slots.get_mut(handle as usize) {
            *slot = None;
        }
    });
}

/// 열린 인스턴스에 접근한다. 핸들이 유효하지 않으면 `None` 을 넘긴 채 호출된다.
pub fn with<T>(handle: i32, f: impl FnOnce(&Instance) -> T) -> Option<T> {
    if handle < 0 {
        return None;
    }
    INSTANCES.with_borrow(|slots| slots.get(handle as usize).and_then(Option::as_ref).map(f))
}

pub fn set_last_error(code: i32) {
    LAST_ERROR.with_borrow_mut(|slot| *slot = code);
}

pub fn last_error() -> i32 {
    LAST_ERROR.with_borrow(|slot| *slot)
}

/// 현재 열려 있는 인스턴스 수. 누수 검사용.
pub fn open_count() -> usize {
    INSTANCES.with_borrow(|slots| slots.iter().filter(|slot| slot.is_some()).count())
}

/// MovieClip 캐릭터의 `MCLP` 본문을 읽는다.
pub fn movie_clip_body(instance: &Instance, character_id: u32) -> Result<MovieClipBody, i32> {
    let entry = instance
        .characters
        .entries
        .get(character_id as usize)
        .ok_or(BAD_ARGUMENT)?;

    if entry.kind != zwf_format::CharacterKind::MovieClip {
        return Err(BAD_ARGUMENT);
    }

    let archive = Archive::parse(&instance.bytes).map_err(|error| error.code())?;
    let mclp = archive
        .find(ChunkId::MCLP)
        .ok_or(Error::MissingRequiredChunk("MCLP"))
        .map_err(|error| error.code())?;
    let payload = mclp.payload().map_err(|error| error.code())?;
    MovieClipBody::parse_at(payload.as_ref(), entry.body_offset, entry.body_size)
        .map_err(|error| error.code())
}

/// MovieClip 캐릭터의 total_frame 을 읽는다.
pub fn movie_clip_total_frame(instance: &Instance, character_id: u32) -> Result<u32, i32> {
    Ok(movie_clip_body(instance, character_id)?.total_frame)
}
