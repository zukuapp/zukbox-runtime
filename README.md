<!-- BEGIN ZUKU OFFICIAL BRAND -->
<!-- markdownlint-disable MD033 MD041 -->
<p align="center">
  <a href="https://docs.zuzunza.com/">
    <picture>
      <source media="(prefers-color-scheme: dark)"
        srcset="docs/branding/zuku-logo-dark.png">
      <img src="docs/branding/zuku-logo-light.png"
        alt="ZUKU" width="320">
    </picture>
  </a>
</p>
<p align="center">ZUKU - 내가 불러 일으키는 새로운 창작.</p>
<!-- markdownlint-enable MD033 MD041 -->
<!-- END ZUKU OFFICIAL BRAND -->

# ZUKBOX ZWF1 런타임

이 저장소는 ZUKBOX의 **ZWF1 바이너리**를 읽는 Rust 파서와 WebAssembly 런타임,
JavaScript 로더를 담습니다. [형식 명세 v0.1](docs/zwf-format-v0.md)은 아직
**초안**입니다. HTML5 게임 ZIP을 패키징하는
[`zwf`의 ZWF2](https://github.com/zukuapp/zwf/blob/main/SPEC.md)와는 확장자만
같고 파일 구조가 다릅니다. npm에서도 ZWF1은 `@zuku/zwf-runtime`,
ZWF2는 `@zuku/zwf`로 구분합니다.

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

공개 npm 배포를 위한 패키지 이름은 `@zuku/zwf-runtime`, 버전은 `0.1.2`입니다.
실제 게시 여부는 npm 레지스트리에서 확인하세요. 설치 패키지는 컴파일된 WASM을
포함하므로 소비자에게 Rust나 설치 후 빌드를 요구하지 않습니다. 소스에서 빌드할
때는 Node.js 22 이상, Rust와 `wasm32-unknown-unknown` 타깃이 필요합니다.

```bash
git clone https://github.com/zukuapp/zukbox-runtime.git
cd zukbox-runtime
rustup target add wasm32-unknown-unknown
npm test
```

`npm test`는 Rust 테스트, WASM 빌드, JavaScript 왕복 테스트를 실행합니다. WASM만
빌드하려면 `npm run build:wasm`을 사용합니다. 산출물은
`target/wasm32-unknown-unknown/release/zwf_runtime.wasm`이고, npm 배포 파일은
`dist/zwf_runtime.wasm`으로 복사됩니다. CI는 빌드 크기가
128 KiB를 넘지 않는지도 검사합니다.

## 호스트에서 읽기

설치한 패키지를 사용하는 예시입니다. `zwfBytes`는 ZWF1 `Uint8Array` 또는
`ArrayBuffer`입니다. 기본 WASM URL은 모듈에 상대적인 배포 파일을 가리킵니다.
브라우저 호스트·번들러는 해당 파일을 함께 제공해야 합니다. Node.js에서는
설치된 파일을 직접 읽습니다. 다른 위치를 쓰면 URL이나 바이트를 명시할 수 있습니다.

```js
import { ZwfRuntime, wasmUrl } from "@zuku/zwf-runtime";

const runtime = await ZwfRuntime.instantiate();
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
유형으로 제공해야 합니다. 다른 MIME 유형은 바이트 로딩으로 처리합니다.
WASM 컴파일을 허용하는 CSP에서는 `script-src 'self' 'wasm-unsafe-eval'`처럼
WebAssembly 전용 허용을 사용하며 JavaScript `eval`은 필요하지 않습니다.
`evalFrame()`이 반환하는 행렬·색상 배열은 복사된 값으로, 다음 호출이나 메모리
해제 후에도 사용할 수 있습니다. 원시 `runtime.exports`의 ABI 포인터·길이·소유권
계약은 여전히 호스트 책임입니다.

The package includes the MIT-licensed ZWF1 WASM and four JavaScript entrypoints.
`ZwfRuntime.instantiate()` loads the installed asset by default in Node.js and
from a module-relative URL in browsers. Serve the asset alongside the module,
or pass an explicit URL, response or byte buffer. `wasmUrl` exposes the asset URL;
the `./wasm` package export also resolves the distributed binary. This remains
the ZWF1 timeline runtime, not the HTML5 ZIP ZWF2 format. Rendering and script
isolation remain host responsibilities; SIGNED is not signature verification.

## 문서와 관련 저장소

- [ZWF1 바이너리 형식 초안](docs/zwf-format-v0.md)
- [파서·ABI와 호스트 보안 경계](docs/safety-and-unsafe.md)
- [ZUKBOX 에디터](https://github.com/zukuapp/zukbox) ·
  [Next2D 플레이어 포크](https://github.com/zukuapp/zukbox-player) ·
  [언어 리소스](https://github.com/zukuapp/zukbox-lang)
- [ZUKU 개발 문서](https://github.com/zukuapp/.github/blob/main/docs/README.md)

코드의 라이선스는 [MIT](LICENSE)입니다.

WASM 배포 빌드는 Rust의 경로 재매핑으로 빌드 호스트의 홈·작업 공간 경로를 제거하고 바이너리에서 재검사합니다. 0.1.2는 이 배포 처리의 패치이며 기존 ZWF1 형식과 JavaScript API는 같습니다. 공개 Rust 표준 라이브러리의 `/rustc/<toolchain-id>/...` 식별자는 개인 호스트 경로가 아닙니다.
