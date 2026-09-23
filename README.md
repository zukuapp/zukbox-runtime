<!-- markdownlint-disable MD033 MD041 -->
<a href="https://zukuapp.github.io/docs/">
  <img
    src="https://raw.githubusercontent.com/zukuapp/.github/main/profile/assets/developer-hero.png"
    alt="Trecillo 로고와 ZUKU 개발자 허브 안내"
    width="760"
  >
</a>
<!-- markdownlint-enable MD033 MD041 -->

# ZUKBOX ZWF1 런타임

이 저장소는 ZUKBOX의 **ZWF1 바이너리**를 읽는 Rust 파서와 WebAssembly 런타임,
JavaScript 로더를 담습니다. [형식 명세 v0.1](docs/zwf-format-v0.md)은 아직
**초안**입니다. HTML5 게임 ZIP을 패키징하는
[`zwf`의 ZWF2](https://github.com/zukuapp/zwf/blob/main/SPEC.md)와는 확장자만
같고 파일 구조가 다릅니다.

## 역할과 현재 범위

```text
ZWF1 파일 → zwf-format (구조 검증·청크 파싱)
          → zwf-runtime (타임라인 평가·렌더 목록)
          → JS 로더와 호스트 렌더러 → 화면
```

| 영역               | 현재 저장소에서 확인되는 내용                                                                |
| ------------------ | -------------------------------------------------------------------------------------------- |
| Rust `zwf-format`  | `no_std` 파서, 헤더·청크 검사, `STAG`·`CHRS`·`MCLP`·`SHAP`·`BMAP`·`VIDS` 처리                |
| Rust `zwf-runtime` | C ABI를 통한 파일 열기·닫기, 스테이지 조회, 타임라인 평가와 렌더 목록                        |
| JavaScript         | [`zwf-loader.mjs`](js/zwf-loader.mjs), [`zwf-player.mjs`](js/zwf-player.mjs), 렌더 큐 도우미 |
| 별도 호스트 책임   | 픽셀 렌더링, 스크립트 실행 여부와 격리 정책, 리소스 제한                                     |

명세에 나오는 `SNDS` 동기, `SIGN` 서명 검증, `ATLS` 패킹 등은 완성된 기능으로
취급하지 마세요. 파일 헤더에 서명 플래그가 있더라도 **암호학적 서명을 검증했다는
뜻은 아닙니다**. 구현 경계는 [안전성 문서](docs/safety-and-unsafe.md)에서
설명합니다.

## 로컬 빌드와 테스트

이 저장소의 `package.json`에는 `private: true`가 설정돼 있습니다.
**`@zukbox/runtime`은 npm에 게시된 설치 패키지가 아닙니다.** 체크아웃한 소스에서
실행하세요. Node.js 22 이상, Rust와 `wasm32-unknown-unknown` 타깃이 필요합니다.

```bash
git clone https://github.com/zukuapp/zukbox-runtime.git
cd zukbox-runtime
rustup target add wasm32-unknown-unknown
npm test
```

`npm test`는 Rust 테스트, WASM 빌드, JavaScript 왕복 테스트를 실행합니다. WASM만
빌드하려면 `npm run build:wasm`을 사용합니다. 산출물은
`target/wasm32-unknown-unknown/release/zwf_runtime.wasm`입니다. CI는 빌드 크기가
128 KiB를 넘지 않는지도 검사합니다.

## 호스트에서 읽기

아래 예시는 **체크아웃한 소스**의 로더를 브라우저 앱에서 가져오는 형태입니다.
앱이 WASM 파일과 ZWF1 파일을 제공하고, `zwfBytes`를 `Uint8Array` 또는
`ArrayBuffer`로 준비해야 합니다.

```js
import { ZwfRuntime } from "./js/zwf-loader.mjs";

const runtime = await ZwfRuntime.instantiate(fetch("/zwf_runtime.wasm"));
const file = runtime.open(zwfBytes);

try {
  console.log(file.stage.width, file.stage.height, file.stage.fps);
  // 렌더링과 신뢰되지 않은 콘텐츠의 격리는 호스트가 구성합니다.
} finally {
  file.close();
}
```

로더는 파일을 열 때 `ZwfError`를 던질 수 있으며, `error.code`는
[`zwf-format` 오류 정의](crates/zwf-format/src/error.rs)에 대응합니다.
브라우저에서 스트리밍 방식으로 WASM을 불러올 때는 서버가 `.wasm`을 올바른 MIME
유형으로 제공해야 합니다.

## 문서와 관련 저장소

- [ZWF1 바이너리 형식 초안](docs/zwf-format-v0.md)
- [파서·ABI와 호스트 보안 경계](docs/safety-and-unsafe.md)
- [ZUKBOX 에디터](https://github.com/zukuapp/zukbox) ·
  [Next2D 플레이어 포크](https://github.com/zukuapp/zukbox-player) ·
  [언어 리소스](https://github.com/zukuapp/zukbox-lang)
- [ZUKU 개발 문서](https://github.com/zukuapp/.github/blob/main/docs/README.md)

코드의 라이선스는 [MIT](LICENSE)입니다.
