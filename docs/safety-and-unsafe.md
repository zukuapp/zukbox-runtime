# ZWF1 파서와 호스트의 안전 경계

이 문서는 저장소 코드의 안전 경계를 정의합니다. ZWF1 파일의 신뢰성이나 웹 서비스
전체의 격리는 이 문서의 범위가 아닙니다.

## 파서와 ABI

[`zwf-format`](../crates/zwf-format/src/lib.rs)은 `no_std` 파서이며
`#![forbid(unsafe_code)]`를 사용합니다. 파서는 헤더와 청크의 구조를 읽고 오류 코드를
반환합니다. 이 검사는 파일 형식 검사입니다. 작성자 신원 확인이나 운영체제
샌드박스가 아닙니다.

[`zwf-runtime`의 C ABI](../crates/zwf-runtime/src/abi.rs)는 JavaScript가 WASM
메모리의 파일 버퍼를 넘기는 경계입니다. 이 파일에는 원시 포인터를 다루는
`unsafe` 코드가 있습니다.

1. 로더는 버퍼를 WASM 메모리에 복사합니다.
2. 로더는 `zwf_open`으로 버퍼 소유권을 넘깁니다.
3. 호스트는 열린 파일을 `close()`로 해제해야 합니다.

ABI 경계를 변경할 때는 포인터 길이, 소유권, 실패 시 해제를 함께 검토해야 합니다.

## 실행하지 않는 것

| 항목                         | 동작                                                         |
| ---------------------------- | ------------------------------------------------------------ |
| 픽셀 렌더링                  | 런타임은 그리지 않습니다. 호스트가 렌더링합니다.             |
| `SCPT` 스크립트 본문         | 런타임은 실행하지 않습니다. 실행 여부와 격리 방식은 호스트가 정합니다. |
| `SIGN` Ed25519 서명 검증     | 구현하지 않습니다. 명세 초안에만 정의돼 있습니다.            |
| 브라우저 권한 분리           | 설정하지 않습니다.                                           |
| 네트워크 접근 제한           | 설정하지 않습니다.                                           |
| CPU·메모리 할당량            | 설정하지 않습니다.                                           |

서명 플래그나 `SIGN` 청크가 있다는 사실은 파일의 신뢰성을 증명하지 않습니다.
서명 플래그나 `SIGN` 청크의 존재만으로 파일을 신뢰하면 안 됩니다.
위 경계가 필요한 호스트는 별도의 정책과 검증을 갖춰야 합니다. HTML5 게임의 **ZWF2**
실행 경계는 [`zwf/SPEC.md`](https://github.com/zukuapp/zwf/blob/main/SPEC.md)를 따릅니다.

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

파서는 청크 색인을 할당하기 전에 실제 파일 길이와 청크 개수를 검사합니다.
JS↔WASM 테스트는 허위 개수와 허위 출력 크기를 메모리 급증이나 트랩 없이 거부하는지
검사합니다. 이 경계는 호스트가 ABI 포인터·길이 쌍의 계약을 지킨다는 전제를 둡니다.
`SIGNED` 플래그는 서명 검증이나 신뢰를 증명하지 않습니다.
