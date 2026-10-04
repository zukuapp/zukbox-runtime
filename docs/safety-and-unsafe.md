# ZWF1 파서와 호스트의 안전 경계

이 문서는 현재 저장소의 코드 경계를 설명합니다. ZWF1 파일을 신뢰할 수 있다고
판단하거나, 웹 서비스 전체의 격리를 보증하는 문서가 아닙니다.

## 파서와 ABI

[`zwf-format`](../crates/zwf-format/src/lib.rs)은 `no_std` 파서이며
`#![forbid(unsafe_code)]`를 사용합니다. 헤더와 청크의 구조를 읽고 오류 코드를
반환합니다. 이 검사는 파일 형식 검사이지 작성자 신원 확인이나 운영체제
샌드박스가 아닙니다.

[`zwf-runtime`의 C ABI](../crates/zwf-runtime/src/abi.rs)는 JavaScript가 WASM
메모리의 파일 버퍼를 넘기는 경계입니다. 이 파일에는 원시 포인터를 다루기 위한
`unsafe` 코드가 있습니다. 로더는 버퍼를 WASM에 복사한 다음 `zwf_open`으로
소유권을 넘기며, 열린 파일은 `close()`로 해제해야 합니다. 경계 변경 시 포인터
길이, 소유권, 실패 시 해제를 함께 검토하세요.

## 실행하지 않는 것

런타임은 픽셀을 그리거나 `SCPT`의 스크립트 본문을 실행하지 않습니다. 실제
렌더링과 신뢰되지 않은 스크립트의 실행 여부·격리 방식은 호스트가 정합니다. 포맷
명세의 `SIGN` 청크는 초안에 정의돼 있지만 현재 저장소는 Ed25519 서명 검증을
제공하지 않습니다. 서명 플래그나 청크의 존재만으로 파일을 신뢰하지 마세요.

이 저장소의 파서와 로더는 브라우저 권한 분리, 네트워크 접근 제한, CPU·메모리
할당량을 설정하지 않습니다. 그런 경계가 필요한 호스트는 별도의 정책과 검증을
갖춰야 합니다. HTML5 게임의 **ZWF2** 실행 경계는 이 문서가 아닌
[`zwf/SPEC.md`](https://github.com/zukuapp/zwf/blob/main/SPEC.md)를 따릅니다.

## Parser and allocation budgets

Before building the chunk index, declared chunk count must fit actual header
space and must not exceed 65,536. Archive and aggregate expanded bytes are
bounded at 512 MiB; an individual chunk is bounded at 128 MiB. DEFLATE must
produce exactly the declared origin size, and padding/trailing-data integrity
is checked. ABI allocation uses a fallible bounded reservation; budget failures
return stable error `-15`, including oversized `zwf_alloc` requests.

청크 색인 할당 전 실제 파일 길이와 개수를 검사합니다. JS↔WASM 테스트는
허위 개수·출력 크기가 메모리 급증이나 트랩 없이 거부되는지 확인합니다.
이 경계는 호스트가 ABI 포인터·길이 쌍의 계약을 지킨다는 전제이며,
`SIGNED` 플래그 자체는 서명 검증이나 신뢰를 증명하지 않습니다.
