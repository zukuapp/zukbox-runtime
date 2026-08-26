# zukbox-runtime

**ZUKBOX `.zwf` 런타임** — Next2D 기반 바이너리 패키지를 어디서든 동일하게 재생하는 WebAssembly 코어.

[![spec](https://img.shields.io/badge/spec-v0.1%20draft-orange)](docs/zwf-format-v0.md)
[![license](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

---

## 이게 무엇인가

ZUKBOX 에디터로 만든 작품은 `.zwf` 파일 하나로 배포된다. 이 저장소는 그 파일을
**읽고 재생하는 쪽**이다.

```
  .zwf (단일 바이너리)
      │
      ▼
  zwf-runtime (wasm)  ──  파싱 · 타임라인 평가 · 행렬/컬러 합성
      │
      ▼  렌더 큐 (WASM 선형 메모리, 복사 없음)
  Next2D 렌더러 워커  ──  WebGL2 / WebGPU
      │
      ▼
  <canvas>
```

### 경계

| | |
|---|---|
| **한다** | `.zwf` 파싱·검증, 타임라인 평가, 변환 합성, 렌더 큐 생성 |
| **하지 않는다** | 픽셀 래스터화(Next2D Player 담당), 스크립트 실행(호스트 샌드박스 담당) |

`.zwf` 를 재생하는 것만으로 임의 코드가 실행되어서는 안 된다 — 명세 §5.9.

---

## 설계 판단

**wasm-bindgen 을 쓰지 않는다.** 순수 C ABI + 손으로 쓴 JS 로더다.

1. 글루 코드 생성 단계가 없어 `WebAssembly.instantiateStreaming` 만으로 로드된다.
2. 산출물이 작다 — 현재 **45 KB**. `.zwf` 첫 재생 지연에 직결된다.
3. 렌더 큐를 선형 메모리에 직접 쓰고 JS 가 `Float32Array` 뷰로 읽는, 복사 없는
   경로를 유지할 수 있다.

**비트맵·오디오·비디오 디코더를 WASM 에 넣지 않는다.** PNG/WebP/AAC 디코딩은
브라우저가 이미 하드웨어 가속으로 한다. `createImageBitmap` 과
`decodeAudioData` 에 맡기는 편이 더 작고 더 빠르다.

**DEFLATE 를 쓴다.** 브라우저 네이티브 `DecompressionStream("deflate-raw")` 로도
풀 수 있어, WASM 없이도 파일을 검증할 수 있는 폴백 경로가 생긴다.

**`unsafe`는 FFI 한 줄에만.** 파서(`zwf-format`)는 `forbid(unsafe_code)`.
런타임의 `unsafe`는 JS↔WASM C ABI(`abi.rs`)에 격리 — wasm-bindgen 없이
제로카피 버퍼를 넘기기 위함이며, UB를 “허용”하는 설계가 아니다.
→ [`docs/safety-and-unsafe.md`](docs/safety-and-unsafe.md)

---

## 구조

```
crates/
  zwf-format/     컨테이너 파싱·검증 (no_std, forbid(unsafe_code))
  zwf-runtime/    WASM cdylib — C ABI 경계
js/
  zwf-loader.mjs  호스트 로더 (프로덕션)
  zwf-writer.mjs  참조 인코더 (테스트 픽스처)
  zwf.test.mjs    JS ↔ WASM 왕복 검증
docs/
  zwf-format-v0.md       포맷 명세 — 이것이 단일 진실
  safety-and-unsafe.md   unsafe/UB 경계 — 왜 ABI만 unsafe인지
```

`zwf-format` 이 `no_std` 인 이유는 브라우저 WASM 과 네이티브 도구(에디터
검증기, CLI)가 **같은 파서**를 쓰게 하기 위해서다. 두 구현이 갈라지면 한쪽만
받아들이는 파일이 생긴다.

---

## 빌드 · 테스트

사전 준비:

```bash
rustup target add wasm32-unknown-unknown
```

> **Windows 참고**: 이 저장소는 `rust-toolchain.toml` 로 툴체인을 고정하지 않는다.
> 고정하면 `channel = "stable"` 이 rustup 의 기본 호스트(보통 `-msvc`)로 해석되는데,
> MSYS2/Git Bash 가 PATH 에 있는 환경에서는 coreutils 의 `link` 가 MSVC `link.exe` 를
> 가려 네이티브 테스트 링크가 깨진다. `-gnu` 호스트 툴체인을 쓰면 그대로 통과한다.
> CI 는 툴체인을 명시적으로 설치하므로 영향이 없다.

전체:

```bash
npm test           # cargo test → wasm 빌드 → JS 왕복 테스트
```

개별:

```bash
npm run test:rust     # cargo test
npm run build:wasm    # → target/wasm32-unknown-unknown/release/zwf_runtime.wasm
npm run test:js       # node --test "js/**/*.test.mjs"
```

JS 왕복 테스트는 **JS 인코더가 쓴 파일을 Rust 파서가 읽게** 한다. 두 구현이
명세를 다르게 읽으면 거기서 걸린다 — 그게 이 테스트의 존재 이유다.

---

## 사용

```js
import { ZwfRuntime } from "@zukbox/runtime";

const runtime = await ZwfRuntime.instantiate(fetch("/zwf_runtime.wasm"));
const file = runtime.open(await (await fetch("/work.zwf")).arrayBuffer());

const { width, height, fps, bgColor } = file.stage;
// … Next2D Player 에 넘겨 재생

file.close();   // WASM 메모리 회수
```

실패는 전부 `ZwfError` 로 온다. `error.code` 는 안정 값이며
`docs/zwf-format-v0.md` 및 `crates/zwf-format/src/error.rs` 와 1:1 대응한다.

---

## 현재 상태

- [x] 컨테이너 헤더·청크 파싱과 검증
- [x] `STAG` / `CHRS` / `MCLP` / `SHAP` / `BMAP` / `VIDS` 디코딩
- [x] 타임라인 평가 (`zwf_eval_frame`)
- [x] C ABI 경계 + JS 로더·플레이어·render-queue
- [x] JS ↔ WASM 왕복 테스트
- [x] 에디터 `.zwf` 퍼블리시 + Loader/render-queue 미리보기
- [ ] Next2D 렌더 큐 **네이티브** 경로 (SHAP recodes 자동 디코딩)
- [ ] `SNDS` 동기
- [ ] `SIGN` Ed25519 검증

---

## 관련 저장소

| 저장소 | 역할 |
|--------|------|
| [zukuapp/zukbox](https://github.com/zukuapp/zukbox) | 에디터 — `Next2D/tool.next2d.app` 포크 |
| [zukuapp/zukbox-player](https://github.com/zukuapp/zukbox-player) | 렌더러 — `Next2D/player` 포크 |
| [zukuapp/zukbox-lang](https://github.com/zukuapp/zukbox-lang) | 언어팩 — `Next2D/language.next2d.app` 포크 |

## 라이선스

MIT. Next2D 프로젝트(MIT, © Toshiyuki Ienaga)의 포맷·런타임 설계에 기반한다.
