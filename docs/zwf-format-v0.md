# ZWF 컨테이너 포맷 명세 v0.1 (Draft)

> **상태**: 초안(Draft) · 미확정. v1.0 고정 전까지 하위 호환을 보장하지 않는다.
> **대상**: `zukbox`(에디터, 쓰기) ↔ `zukbox-runtime`(WASM, 읽기) 사이의 유일한 계약.
> **최종 갱신**: 2026-08-23

---

## 1. 설계 목표

| 목표 | 이유 |
|------|------|
| **단일 파일 바이너리** | `.zwf` 하나만 배포하면 모바일/PC/태블릿 어디서든 동일 재생 (요구사항 9) |
| **제로카피 파싱** | 청크 페이로드를 WASM 선형 메모리에 그대로 두고 슬라이스로 참조. 대용량 에셋에서 GC 압력 없음 |
| **스트리밍 가능** | 청크가 의존 순서대로 정렬되면 다운로드 중 순차 파싱 가능 (`flags` bit1) |
| **부분 호환** | 모르는 청크에 `SKIPPABLE` 플래그가 있으면 구버전 런타임이 건너뛰고 재생 지속 |
| **툴체인 비의존** | wasm-bindgen 없이 순수 C ABI. `cargo build --target wasm32-unknown-unknown` 만으로 산출 |

**비목표(v0)**: 실행 중 부분 로드(ID 기반 랜덤 액세스), 암호화, 델타 패치.

---

## 2. 공통 규약

- **바이트 순서**: 전부 **리틀 엔디언**.
- **정렬**: 모든 청크 헤더는 **4바이트 경계**에서 시작한다. 페이로드 뒤에 0~3바이트의 `0x00` 패딩을 넣어 맞춘다. 패딩은 `stored_size`에 포함되지 않는다.
- **FourCC**: ASCII 4바이트. 4자 미만이면 공백(`0x20`)으로 우측 패딩 (예: `"SND "`).
- **문자열**: 길이 접두(`u32`) + UTF-8 바이트. NUL 종료 없음.
- **행렬**: `[a, b, c, d, tx, ty]` — `f32` 6개. Next2D `Matrix`와 동일 순서.
- **컬러 트랜스폼**: `[rM, gM, bM, aM, rO, gO, bO, aO]` — `f32` 8개. Next2D `ColorTransform`과 동일 순서.
- **CRC-32**: IEEE 802.3 다항식(`0xEDB88320`, reflected).

---

## 3. 파일 헤더 (32바이트, 고정)

| 오프셋 | 크기 | 필드 | 값/설명 |
|--------|------|------|---------|
| 0 | 4 | `magic` | `"ZWF1"` = `5A 57 46 31` |
| 4 | 1 | `version_major` | `0` |
| 5 | 1 | `version_minor` | `1` |
| 6 | 2 | `flags` | 아래 표 |
| 8 | 4 | `header_size` | `32` (미래 확장 시 증가) |
| 12 | 4 | `chunk_count` | 청크 총 개수 |
| 16 | 8 | `file_size` | 패딩 포함 파일 전체 바이트 수 |
| 24 | 4 | `reserved` | `0` |
| 28 | 4 | `header_crc32` | 바이트 `[0..28)` 의 CRC-32 |

### 파일 플래그

| 비트 | 이름 | 의미 |
|------|------|------|
| 0 | `SIGNED` | `SIGN` 청크가 마지막에 존재 |
| 1 | `STREAMABLE` | 청크가 의존 순서로 정렬됨 (§6) |
| 2 | `ATLAS_PACKED` | 비트맵이 `ATLS` 텍스처 아틀라스로 병합됨 |
| 3–15 | — | 예약, `0` |

**검증 순서**: `magic` → `header_crc32` → `version_major <= 지원버전` → `file_size`가 실제 길이와 일치.
`version_major`가 런타임 지원치를 넘으면 **즉시 거부**한다(재생 시도 금지).

---

## 4. 청크 (TLV)

각 청크는 16바이트 헤더 + 페이로드 + 정렬 패딩.

| 오프셋 | 크기 | 필드 | 설명 |
|--------|------|------|------|
| 0 | 4 | `id` | FourCC |
| 4 | 1 | `codec` | `0`=RAW, `1`=DEFLATE(RFC 1951, raw), `2`~=예약 |
| 5 | 1 | `flags` | bit0 `SKIPPABLE`, bit1 `HAS_CRC` |
| 6 | 2 | `reserved` | `0` |
| 8 | 4 | `stored_size` | 디스크상 페이로드 바이트 수 (패딩 제외) |
| 12 | 4 | `origin_size` | 압축 해제 후 바이트 수. `codec=RAW`이면 `stored_size`와 같아야 함 |

`flags.HAS_CRC`가 서면 페이로드 **뒤에** `u32` CRC-32(원본 바이트 기준)가 붙고, 이 4바이트는 `stored_size`에 **포함되지 않으며** 패딩 계산 전에 위치한다.

> **DEFLATE 선택 이유**: 브라우저 네이티브 `DecompressionStream("deflate-raw")`로도 풀 수 있어, JS 폴백 경로에서 WASM 없이 검증이 가능하다. 런타임 내부는 `miniz_oxide`(순수 Rust)를 써서 모듈을 자기완결적으로 유지한다.

---

## 5. 청크 정의

### 5.1 `META` — 메타데이터 *(필수, 첫 청크)*

UTF-8 JSON. 사람이 읽을 수 있어야 하며, 재생에 필수적인 값은 여기 두지 않는다(그건 `STAG`).

```json
{
  "title": "작품 제목",
  "author": "제작자",
  "tool": "zukbox/0.1.0",
  "created": "2026-08-23T04:00:00Z",
  "locale": "ko-KR",
  "runtimeMin": "0.1.0"
}
```

### 5.2 `STAG` — 스테이지 *(필수, 24바이트 고정)*

에디터 `IStageObject`와 1:1 대응.

| 오프셋 | 타입 | 필드 | 비고 |
|--------|------|------|------|
| 0 | `u32` | `width` | px |
| 4 | `u32` | `height` | px |
| 8 | `f32` | `fps` | |
| 12 | `u32` | `bg_rgba` | `0xRRGGBBAA`. 에디터의 `bgColor` 문자열을 여기서 정규화 |
| 16 | `u32` | `root_character_id` | 루트 MovieClip |
| 20 | `u32` | `reserved` | `0` |

### 5.3 `SYMB` — 심볼 테이블 *(선택)*

`IPublishObject.symbols`(`Array<[string, number]>`)에 대응. 스크립트에서 이름으로 캐릭터를 찾을 때만 필요.

```
u32 count
count × { u32 name_len; u8[name_len] name_utf8; u32 character_id }   // 각 엔트리 4바이트 정렬
```

### 5.4 `CHRS` — 캐릭터 색인 *(필수)*

`IPublishObject.characters` 배열의 타입 디스패치 테이블. **배열 인덱스가 곧 `characterId`.**

```
u32 count
count × {
  u8  kind        // 1=MOVIE_CLIP, 2=SHAPE, 3=BITMAP, 4=VIDEO, 5=TEXT
  u8  flags       // bit0 = exported (SYMB에 이름 있음)
  u16 reserved
  u32 body_offset // 해당 kind의 본문 청크(MCLP/SHAP/BMAP/VIDS) 내 바이트 오프셋
  u32 body_size
}
```

### 5.5 `MCLP` — MovieClip 타임라인 *(필수)*

`IMovieClipPublishJson`에 대응. 포맷의 핵심이며 런타임 성능을 좌우한다.

각 MovieClip 본문:

```
u32 total_frame
u32 dictionary_count
dictionary_count × {           // ICharacterPublishObject
  u32 character_id
  u32 start_frame
  u32 end_frame
  i32 clip_depth               // 없으면 -1
}
u32 depth_count                // controller/placeMap 의 depth 축 크기
u32 place_object_count
place_object_count × PlaceObject
u32 controller_len             // depth_count × total_frame 개
controller_len × i32           // null = -1 (dictionary 인덱스)
u32 place_map_len              // depth_count × total_frame 개
place_map_len   × i32          // null = -1 (placeObjects 인덱스)
```

`controller`와 `placeMap`은 에디터에서 `{[depth]: (number|null)[]}` 형태의 희소 맵이지만, **런타임에서는 `depth × frame` 밀집 `i32` 행렬로 평탄화**한다. 프레임 전진 시 행 단위 순차 접근이 되어 캐시 지역성이 크게 좋아진다. `null`은 `-1`로 인코딩한다.

#### PlaceObject (가변 길이)

```
u8  present       // bit0 matrix, bit1 colorTransform, bit2 blendMode, bit3 filters, bit4 loop
u8  blend_mode    // present.bit2 일 때만 유효. 문자열 → §7 열거값
u16 filter_count  // present.bit3 일 때만 유효
[f32 × 6]  matrix           // present.bit0
[f32 × 8]  colorTransform   // present.bit1
filter_count × { u8 filter_class; u8 param_count; u16 pad; f32[param_count] params }
[ u8 loop_type; u8 pad3[3]; u32 start_frame; u32 frame_count ]   // present.bit4
```

### 5.6 `SHAP` — 벡터 셰이프 *(선택)*

`IShapePublishJson`. `recodes`는 Next2D 렌더 커맨드 배열이며, **v0에서는 `f32` 평탄 배열로 그대로 직렬화**한다(에디터가 이미 이 형태로 생성). 헤더에 bounds/grid를 둔다.

```
f32 x_min, x_max, y_min, y_max
u8  flags            // bit0 = has_grid, bit1 = in_bitmap, bit2 = has_bitmap_id
u8  pad[3]
[ f32 grid_x, grid_y, grid_w, grid_h ]   // flags.bit0
[ u32 bitmap_id ]                        // flags.bit2
u32 recode_len
recode_len × f32
```

### 5.7 `BMAP` / `ATLS` — 비트맵 *(선택)*

`IBitmapPublishJson`. 에디터의 `buffer`는 RGBA 픽셀 배열(`number[]`)이다. **v0에서는 원본 인코딩(PNG/WebP) 바이트를 그대로 저장**하고 디코딩은 브라우저 `createImageBitmap`에 맡긴다 — WASM에 PNG 디코더를 넣는 것보다 작고 빠르다.

```
f32 x_min, x_max, y_min, y_max
u8  encoding      // 0=RAW_RGBA8, 1=PNG, 2=WEBP, 3=AVIF
u8  pad[3]
u32 byte_len
u8[byte_len] data
```

`ATLAS_PACKED` 플래그가 선 경우 `ATLS` 청크가 병합 텍스처를 담고, `BMAP` 엔트리는 아틀라스 내 UV 사각형만 갖는다. (v0 미구현)

### 5.8 `VIDS` / `SNDS` — 비디오·사운드 *(선택)*

컨테이너 바이트를 그대로 담고 디코딩은 플랫폼(`HTMLVideoElement` / `AudioContext.decodeAudioData`)에 위임한다. `IVideoPublishJson`의 `volume`/`loop`/`autoPlay`, `ISoundPublishObject`의 `frame` 동기 정보를 헤더에 둔다.

### 5.9 `SCPT` — 프레임 액션 *(선택)*

`IActionSaveObject[]` + `ILabelSaveObject[]`. 스크립트 본문은 UTF-8 소스 문자열.

> **보안 경계**: 런타임 WASM은 스크립트를 **실행하지 않는다**. 파싱해서 호스트로 넘길 뿐이며, 실행은 호스트가 정한 샌드박스 정책을 따른다. `.zwf` 재생만으로 임의 코드가 실행되어서는 안 된다.

### 5.10 `SIGN` — 서명 *(선택, 반드시 마지막)*

```
u8  algorithm      // 1 = Ed25519
u8  pad[3]
u32 key_id
u32 sig_len
u8[sig_len] signature     // [0 .. SIGN 청크 헤더 시작) 전체 바이트에 대한 서명
```

---

## 6. 청크 순서

`STREAMABLE`이 설정된 파일은 다음 순서를 지켜야 한다.

```
META → STAG → SYMB → CHRS → SHAP → BMAP/ATLS → MCLP → SNDS → VIDS → SCPT → SIGN
```

근거: 스테이지 크기를 먼저 알아야 캔버스를 띄우고, 셰이프·비트맵이 준비되어야 첫 프레임을 그릴 수 있으며, 사운드·비디오는 첫 프레임 이후 도착해도 된다.

---

## 7. 열거형

### blend_mode

| 값 | 이름 | 값 | 이름 |
|----|------|----|------|
| 0 | normal | 7 | invert |
| 1 | layer | 8 | alpha |
| 2 | multiply | 9 | erase |
| 3 | screen | 10 | overlay |
| 4 | lighten | 11 | hardlight |
| 5 | darken | 12 | add |
| 6 | difference | 13 | subtract |

### filter_class

| 값 | 클래스 | 값 | 클래스 |
|----|--------|----|--------|
| 0 | BevelFilter | 5 | GlowFilter |
| 1 | BlurFilter | 6 | GradientBevelFilter |
| 2 | ColorMatrixFilter | 7 | GradientGlowFilter |
| 3 | ConvolutionFilter | 8 | DisplacementMapFilter |
| 4 | DropShadowFilter | | |

---

## 8. 에디터 JSON → ZWF 매핑 요약

| 에디터 (`IPublishObject`) | ZWF |
|---------------------------|-----|
| `stage` | `STAG` |
| `symbols` | `SYMB` |
| `characters[]` (디스패치) | `CHRS` |
| `characters[]` `extends: "MovieClip"` | `MCLP` |
| `characters[]` `extends: "Shape"` | `SHAP` |
| `characters[]` `extends: "Bitmap"` | `BMAP` |
| `characters[]` `extends: "Video"` | `VIDS` |
| `MovieClip.sounds` | `SNDS` |
| `MovieClip.actions` / `labels` | `SCPT` |
| `type: "json"` | (불필요 — 매직 넘버가 대체) |

---

## 9. 미결 사항 (v1.0 전 확정 필요)

1. `SHAP.recodes`의 실제 커맨드 스키마 — 현재는 Next2D 내부 표현에 암묵 의존한다. 자체 opcode 표로 고정해야 업스트림 변경에 안 깨진다.
2. 텍스트(`kind=5`) 청크 미정의 — 폰트 서브셋 임베딩 방식 포함.
3. `ATLS` 아틀라스 패킹 규칙.
4. `SIGN` 키 배포·폐기(revocation) 절차.
5. 대용량 파일(>4GiB) — 현재 `stored_size`가 `u32`라 청크당 4GiB 상한.
