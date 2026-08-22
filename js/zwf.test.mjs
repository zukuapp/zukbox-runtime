/**
 * 호스트 ↔ WASM 왕복 검증.
 *
 * JS 인코더가 만든 파일을 Rust 파서가 읽어낸다. 두 구현이 명세를 서로 다르게
 * 읽으면 여기서 걸린다 — 그게 이 테스트의 존재 이유다.
 *
 *   node --test js/
 */

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { test, before } from "node:test";

import { ZwfRuntime, ZwfError } from "./zwf-loader.mjs";
import { ZwfWriter, FILE_FLAGS, CHUNK_FLAGS, crc32, encodeStage, minimalFile, sampleTimelineFile } from "./zwf-writer.mjs";

const WASM_PATH = fileURLToPath(
    new URL("../target/wasm32-unknown-unknown/release/zwf_runtime.wasm", import.meta.url)
);

let runtime;

before(async () =>
{
    let bytes;
    try {
        bytes = await readFile(WASM_PATH);
    } catch {
        throw new Error(
            `WASM 이 없습니다: ${WASM_PATH}\n` +
            "먼저 빌드하세요: cargo build --release --target wasm32-unknown-unknown -p zwf-runtime"
        );
    }
    runtime = await ZwfRuntime.instantiate(bytes);
});

test("명세 버전이 0.1 이다", () =>
{
    assert.deepEqual(runtime.specVersion, { "major": 0, "minor": 1 });
});

test("CRC-32 가 표준 벡터와 일치한다", () =>
{
    // Rust 쪽 crc32::tests::known_vectors 와 같은 벡터.
    assert.equal(crc32(new TextEncoder().encode("")), 0x00000000);
    assert.equal(crc32(new TextEncoder().encode("a")), 0xe8b7be43);
    assert.equal(crc32(new TextEncoder().encode("123456789")), 0xcbf43926);
});

test("JS 가 쓴 파일을 WASM 이 읽고 스테이지를 되돌려준다", () =>
{
    const bytes = minimalFile({
        "width": 1920,
        "height": 1080,
        "fps": 30,
        "bgRgba": 0x1e1e1eff
    });

    const file = runtime.open(bytes);
    try {
        assert.equal(file.stage.width, 1920);
        assert.equal(file.stage.height, 1080);
        assert.equal(file.stage.fps, 30);
        assert.equal(file.stage.bgColor, "#1e1e1e");
        assert.equal(file.stage.bgAlpha, 1);
        assert.deepEqual(file.chunkIds, ["META", "STAG", "CHRS", "MCLP"]);
        assert.deepEqual(file.flags, { "signed": false, "streamable": true });
    } finally {
        file.close();
    }
});

test("청크 CRC 가 붙어 있어도 그대로 읽힌다", () =>
{
    const bytes = new ZwfWriter(FILE_FLAGS.STREAMABLE)
        .pushRaw("META", new TextEncoder().encode("{}"), CHUNK_FLAGS.HAS_CRC)
        .pushRaw("STAG", encodeStage({ "width": 640, "height": 480, "fps": 12 }), CHUNK_FLAGS.HAS_CRC)
        .pushRaw("CHRS", new Uint8Array(4))
        .pushRaw("MCLP", new Uint8Array(0))
        .finish();

    const file = runtime.open(bytes);
    try {
        assert.equal(file.stage.width, 640);
        assert.equal(file.stage.fps, 12);
    } finally {
        file.close();
    }
});

test("ZWF 가 아닌 바이트는 매직 넘버에서 거부된다", () =>
{
    assert.throws(
        () => runtime.open(new Uint8Array(64).fill(0xab)),
        (error) => error instanceof ZwfError && error.code === -2
    );
});

test("한 바이트만 뒤집혀도 헤더 CRC 에서 잡힌다", () =>
{
    const bytes = minimalFile();
    bytes[12] ^= 0xff; // chunk_count 손상
    assert.throws(
        () => runtime.open(bytes),
        (error) => error instanceof ZwfError && error.code === -3
    );
});

test("필수 청크가 빠지면 거부된다", () =>
{
    const bytes = new ZwfWriter(0)
        .pushRaw("META", new TextEncoder().encode("{}"))
        .pushRaw("STAG", encodeStage({ "width": 1, "height": 1, "fps": 1 }))
        .finish(); // CHRS, MCLP 없음

    assert.throws(
        () => runtime.open(bytes),
        (error) => error instanceof ZwfError && error.code === -13
    );
});

test("잘린 파일은 길이 검사에서 걸린다", () =>
{
    const bytes = minimalFile();
    assert.throws(
        () => runtime.open(bytes.subarray(0, bytes.length - 8)),
        (error) => error instanceof ZwfError && error.code === -6
    );
});

test("모든 실패 경로에서 인스턴스가 새지 않는다", () =>
{
    const before = runtime.openCount;

    for (const bad of [
        new Uint8Array(8),                      // 너무 짧음
        new Uint8Array(64).fill(0xab),          // 매직 불일치
        minimalFile().subarray(0, 40)           // 잘림
    ]) {
        assert.throws(() => runtime.open(bad));
    }

    assert.equal(runtime.openCount, before);
});

test("CHRS/MCLP 타임라인을 WASM 이 읽는다", () =>
{
    const bytes = sampleTimelineFile();
    const file = runtime.open(bytes);
    try {
        assert.equal(file.characterCount, 1);
        assert.equal(file.mclipTotalFrame(0), 1);
    } finally {
        file.close();
    }
});

test("evalFrame 이 타임라인 프레임의 렌더 목록을 반환한다", () =>
{
    const bytes = sampleTimelineFile();
    const file = runtime.open(bytes);
    try {
        const list = file.evalFrame(0, 0);
        assert.equal(list.length, 1);
        assert.equal(list[0].depth, 0);
        assert.equal(list[0].characterId, 1);
        assert.deepEqual([...list[0].matrix], [1, 0, 0, 1, 100, 50]);
        assert.deepEqual(
            [...list[0].colorTransform],
            [1, 0, 0, 1, 0, 0, 0, 1]
        );
        assert.equal(list[0].blendMode, 0);
    } finally {
        file.close();
    }
});

test("close 이후에는 접근이 막힌다", () =>
{
    const file = runtime.open(minimalFile());
    file.close();

    assert.equal(file.closed, true);
    assert.throws(() => file.stage, (error) => error instanceof ZwfError && error.code === -101);

    // 두 번 닫아도 안전해야 한다.
    file.close();
});
