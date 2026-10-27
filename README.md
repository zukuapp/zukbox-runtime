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

![version](https://img.shields.io/badge/version-v27.26102701-informational)

## 개요

이 저장소는 ZUKBOX `ZWF1` 바이너리를 읽는 Rust 파서, WebAssembly 런타임,
JavaScript 로더를 담습니다. 형식은 [형식 명세 v0.1 초안](docs/zwf-format-v0.md)을
따릅니다.

`ZWF1`과 [`zwf`의 ZWF2](https://github.com/zukuapp/zwf/blob/main/SPEC.md)는
확장자만 같고 파일 구조가 다릅니다. ZWF2는 HTML5 게임 ZIP 형식입니다.
npm 패키지는 ZWF1이 `@zuku/zwf-runtime`, ZWF2가 `@zuku/zwf`입니다.

```text
ZWF1 파일 → zwf-format (구조 검증·청크 파싱)
          → zwf-runtime (타임라인 평가·렌더 목록)
          → JS 로더와 호스트 렌더러 → 화면
```

| 영역               | 구현 범위                                                                                    |
| ------------------ | -------------------------------------------------------------------------------------------- |
| Rust `zwf-format`  | `no_std` 파서, 헤더·청크 검사, `STAG`·`CHRS`·`MCLP`·`SHAP`·`BMAP`·`VIDS` 처리                |
| Rust `zwf-runtime` | C ABI를 통한 파일 열기·닫기, 스테이지 조회, 타임라인 평가와 렌더 목록                        |
| JavaScript         | [`zwf-loader.mjs`](js/zwf-loader.mjs), [`zwf-player.mjs`](js/zwf-player.mjs), 렌더 큐 도우미 |
| 별도 호스트 책임   | 픽셀 렌더링, 스크립트 실행 여부와 격리 정책, 리소스 제한                                     |

런타임은 명세의 `SNDS` 동기, `SIGN` 서명 검증, `ATLS` 패킹을 구현하지 않습니다.
파일 헤더의 `SIGNED` 플래그는 서명 검증 결과가 아닙니다.

## 요구 사항

| 용도            | 요구 사항                                          |
| --------------- | -------------------------------------------------- |
| 패키지 사용     | Node.js 22 이상 또는 WebAssembly를 지원하는 브라우저 |
| 소스에서 빌드   | Node.js 22 이상, Rust, `wasm32-unknown-unknown` 타깃 |

설치 패키지는 컴파일된 WASM을 포함합니다.

## 설치

npm 패키지 이름은 `@zuku/zwf-runtime`입니다. `package.json`의 패키지 버전은
`0.1.2`입니다.

```bash
npm install @zuku/zwf-runtime
```

소스에서 빌드하고 테스트하는 절차는 다음과 같습니다.

1. 저장소를 클론합니다.
2. WASM 타깃을 추가합니다.
3. 테스트를 실행합니다.

```bash
git clone https://github.com/zukuapp/zukbox-runtime.git
cd zukbox-runtime
rustup target add wasm32-unknown-unknown
npm test
```

| 명령                 | 동작                                                         |
| -------------------- | ------------------------------------------------------------ |
| `npm test`           | Rust 테스트, WASM 빌드, JavaScript 왕복 테스트를 실행합니다. |
| `npm run build:wasm` | WASM만 빌드합니다.                                           |
| `npm run test:packed`| `npm pack` 산출물을 새 디렉터리에 설치해 검사합니다.         |

빌드 산출물은 `target/wasm32-unknown-unknown/release/zwf_runtime.wasm`입니다.
npm 배포 파일은 `dist/zwf_runtime.wasm`으로 복사됩니다.
CI는 WASM 크기가 128 KiB 이하인지 검사합니다.

## 사용법

`zwfBytes`는 ZWF1 데이터를 담은 `Uint8Array` 또는 `ArrayBuffer`입니다.

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

- `runtime.open()`은 파일을 열지 못하면 `ZwfError`를 던집니다.
  `error.code`는 [`zwf-format` 오류 정의](crates/zwf-format/src/error.rs)에 대응합니다.
- `evalFrame()`이 반환하는 행렬·색상 배열은 복사본입니다. 다음 호출이나 메모리
  해제 후에도 값이 유지됩니다.
- 원시 `runtime.exports`의 ABI 포인터·길이·소유권 계약은 호스트 책임입니다.

패키지는 다음 진입점을 제공합니다.

| 경로                              | 내용                          |
| --------------------------------- | ----------------------------- |
| `@zuku/zwf-runtime`               | 로더 (`ZwfRuntime`, `ZwfError`, `wasmUrl`) |
| `@zuku/zwf-runtime/writer`        | ZWF1 작성 도우미              |
| `@zuku/zwf-runtime/player`        | 플레이어                      |
| `@zuku/zwf-runtime/render-queue`  | 렌더 큐 도우미                |
| `@zuku/zwf-runtime/wasm`          | 배포된 WASM 바이너리          |

## 설정

`ZwfRuntime.instantiate(source)`의 `source`로 WASM 위치를 지정합니다.

| `source` 값                          | 동작                                                          |
| ------------------------------------ | ------------------------------------------------------------- |
| 생략                                 | 기본값 `wasmUrl`을 사용합니다.                                |
| `string`, `URL`                      | Node.js의 `file:` URL은 파일을 직접 읽고, 그 외에는 `fetch`로 받습니다. |
| `Response`, `Promise<Response>`      | 응답 본문으로 WASM을 컴파일합니다.                            |
| `BufferSource`                       | 바이트로 WASM을 컴파일합니다.                                 |

`wasmUrl`은 모듈 기준 상대 경로의 `dist/zwf_runtime.wasm`을 가리킵니다.
Node.js에서는 설치된 파일을 직접 읽습니다. 브라우저 호스트와 번들러는 이 파일을
모듈과 함께 제공합니다.

`npm run test:packed`는 다음 환경 변수가 모두 설정되면 Chrome 브라우저 검사도 실행합니다.
설정되지 않으면 브라우저 검사를 건너뜁니다.

| 환경 변수                          | 값                                  |
| ---------------------------------- | ----------------------------------- |
| `ZUKU_RUNTIME_PLAYWRIGHT_MODULE`   | Playwright 모듈 경로                |
| `ZUKU_RUNTIME_BROWSER_EXECUTABLE`  | Chrome 실행 파일 경로               |

## 문제 해결

| 증상                                 | 원인과 조치                                                                 |
| ------------------------------------ | --------------------------------------------------------------------------- |
| 스트리밍 컴파일이 쓰이지 않음        | 서버가 `.wasm`을 `application/wasm`으로 제공해야 합니다. 다른 MIME 유형은 바이트 로딩으로 처리합니다. |
| CSP가 WASM 컴파일을 차단함           | `script-src 'self' 'wasm-unsafe-eval'`을 허용합니다. |
| `WASM request failed (HTTP …)` 오류  | WASM URL이 실패 응답을 반환했습니다. 파일 배포 경로를 확인합니다.           |

## 문서와 관련 저장소

- [ZWF1 바이너리 형식 초안](docs/zwf-format-v0.md)
- [파서·ABI와 호스트 보안 경계](docs/safety-and-unsafe.md)
- [ZUKBOX 에디터](https://github.com/zukuapp/zukbox) ·
  [Next2D 플레이어 포크](https://github.com/zukuapp/zukbox-player) ·
  [언어 리소스](https://github.com/zukuapp/zukbox-lang)
- [ZUKU 개발 문서](https://github.com/zukuapp/.github/blob/main/docs/README.md)

## English summary

The package includes the MIT-licensed ZWF1 WASM and four JavaScript entrypoints.
`ZwfRuntime.instantiate()` loads the installed asset in Node.js.
In browsers, it loads the asset from a module-relative URL.
Serve the asset alongside the module, or pass a URL, response, or byte buffer.
`wasmUrl` exposes the asset URL.
The `./wasm` package export resolves the distributed binary.
ZWF2, the HTML5 game ZIP format, uses the `@zuku/zwf` package.
The host is responsible for rendering and script isolation.
The `SIGNED` flag is not signature verification.

## 변경 이력

변경 이력은 [CHANGELOG](CHANGELOG.md)에 기록합니다.

## 라이선스

코드의 라이선스는 [MIT](LICENSE)입니다.
