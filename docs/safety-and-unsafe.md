# ZWF1 파서와 호스트의 안전 경계

## 파서와 ABI

[`zwf-format`](../crates/zwf-format/src/lib.rs)은 `no_std` 파서이며
`#![forbid(unsafe_code)]`를 사용합니다. 파서는 헤더와 청크의 구조를 읽고 오류 코드를
반환합니다.

[`zwf-runtime`의 C ABI](../crates/zwf-runtime/src/abi.rs)는 JavaScript가 WASM
메모리의 파일 버퍼를 넘기는 경계입니다. 이 파일에는 원시 포인터를 다루는
`unsafe` 코드가 있습니다.

1. 로더는 버퍼를 WASM 메모리에 복사합니다.
2. 로더는 `zwf_open`으로 버퍼 소유권을 넘깁니다.
3. 호스트는 열린 파일을 `close()`로 해제해야 합니다.

ABI 경계를 변경할 때는 포인터 길이, 소유권, 실패 시 해제를 함께 검토해야 합니다.

## 호스트 책임

다음 항목은 호스트가 구현하거나 설정합니다.

| 항목                         | 호스트의 역할                                    |
| ---------------------------- | ------------------------------------------------ |
| 픽셀 렌더링                  | 렌더 목록을 받아 화면에 그립니다.                |
| `SCPT` 스크립트 본문         | 실행 여부와 격리 방식을 정합니다.                |
| `SIGN` Ed25519 서명 검증     | 서명이 필요하면 직접 검증합니다.                 |
| 브라우저 권한 분리           | 호스트 정책으로 설정합니다.                      |
| 네트워크 접근 제한           | 호스트 정책으로 설정합니다.                      |
| CPU·메모리 할당량            | 호스트 정책으로 설정합니다.                      |

서명 플래그나 `SIGN` 청크의 존재만으로 파일을 신뢰하면 안 됩니다.

## Parser and allocation budgets

Before building the chunk index, the parser checks that the declared chunk count
fits the actual header space and does not exceed 65,536.

| Limit                                   | Value     |
| --------------------------------------- | --------- |
| Archive and aggregate expanded bytes    | 512 MiB   |
| Individual chunk                        | 128 MiB   |
| Chunk count                             | 65,536    |

DEFLATE output must match the declared origin size exactly.
The parser checks padding and trailing-data integrity.
ABI allocation uses a fallible bounded reservation.
Budget failures return stable error `-15`, including oversized `zwf_alloc` requests.

호스트는 ABI 포인터·길이 쌍의 계약을 지켜야 합니다.
