/**
 * 최소 ZWF 인코더 — 테스트 픽스처 생성용.
 *
 * 진짜 퍼블리시 경로는 에디터(zukbox) 안에 들어간다. 여기 있는 것은
 * 런타임을 검증할 파일을 만들기 위한 참조 구현이며, 명세를 두 언어로
 * 각각 구현해 서로를 교차 검증하는 역할도 한다.
 */

const CHUNK_HEADER_SIZE = 16;
const FILE_HEADER_SIZE = 32;

export const CODEC = Object.freeze({ "RAW": 0, "DEFLATE": 1 });
export const CHUNK_FLAGS = Object.freeze({ "SKIPPABLE": 1, "HAS_CRC": 2 });
export const FILE_FLAGS = Object.freeze({ "SIGNED": 1, "STREAMABLE": 2, "ATLAS_PACKED": 4 });

const CRC_TABLE = (() =>
{
    const table = new Uint32Array(256);
    for (let idx = 0; idx < 256; ++idx) {
        let crc = idx;
        for (let bit = 0; bit < 8; ++bit) {
            crc = crc & 1 ? (crc >>> 1) ^ 0xedb88320 : crc >>> 1;
        }
        table[idx] = crc >>> 0;
    }
    return table;
})();

/** CRC-32 (IEEE, reflected) — Rust 쪽 `crc32::checksum` 과 같은 값을 내야 한다. */
export const crc32 = (bytes) =>
{
    let crc = 0xffffffff;
    for (let idx = 0; idx < bytes.length; ++idx) {
        crc = (crc >>> 8) ^ CRC_TABLE[(crc ^ bytes[idx]) & 0xff];
    }
    return (crc ^ 0xffffffff) >>> 0;
};

const alignUp = (value) => Math.ceil(value / 4) * 4;

const fourCC = (id) =>
{
    const padded = id.padEnd(4, " ");
    return Uint8Array.from([
        padded.charCodeAt(0), padded.charCodeAt(1),
        padded.charCodeAt(2), padded.charCodeAt(3)
    ]);
};

/** `STAG` 페이로드를 만든다 — 명세 §5.2. */
export const encodeStage = ({ width, height, fps, bgRgba = 0x000000ff, rootCharacterId = 0 }) =>
{
    const payload = new Uint8Array(24);
    const view = new DataView(payload.buffer);
    view.setUint32(0, width, true);
    view.setUint32(4, height, true);
    view.setFloat32(8, fps, true);
    view.setUint32(12, bgRgba, true);
    view.setUint32(16, rootCharacterId, true);
    return payload;
};

export class ZwfWriter
{
    /**
     * @param {number} flags
     */
    constructor (flags = FILE_FLAGS.STREAMABLE)
    {
        this._flags = flags;
        this._parts = [];
        this._length = 0;
        this._chunkCount = 0;
    }

    _append (bytes)
    {
        this._parts.push(bytes);
        this._length += bytes.length;
    }

    /**
     * 무압축 청크를 덧붙인다.
     *
     * @param {string} id FourCC
     * @param {Uint8Array} payload
     * @param {number} chunkFlags
     */
    pushRaw (id, payload, chunkFlags = 0)
    {
        const header = new Uint8Array(CHUNK_HEADER_SIZE);
        header.set(fourCC(id), 0);
        header[4] = CODEC.RAW;
        header[5] = chunkFlags;

        const view = new DataView(header.buffer);
        view.setUint32(8, payload.length, true);
        view.setUint32(12, payload.length, true);

        this._append(header);
        this._append(payload);

        if (chunkFlags & CHUNK_FLAGS.HAS_CRC) {
            const crc = new Uint8Array(4);
            new DataView(crc.buffer).setUint32(0, crc32(payload), true);
            this._append(crc);
        }

        // 다음 청크는 4바이트 경계에서 시작해야 한다 — 명세 §2.
        const padding = alignUp(this._length) - this._length;
        if (padding > 0) {
            this._append(new Uint8Array(padding));
        }

        this._chunkCount += 1;
        return this;
    }

    /** @returns {Uint8Array} */
    finish ()
    {
        const fileSize = FILE_HEADER_SIZE + this._length;

        const header = new Uint8Array(FILE_HEADER_SIZE);
        header.set(fourCC("ZWF1"), 0);
        header[4] = 0; // version_major
        header[5] = 1; // version_minor

        const view = new DataView(header.buffer);
        view.setUint16(6, this._flags, true);
        view.setUint32(8, FILE_HEADER_SIZE, true);
        view.setUint32(12, this._chunkCount, true);
        view.setBigUint64(16, BigInt(fileSize), true);
        // 24..28 은 reserved.
        view.setUint32(28, crc32(header.subarray(0, 28)), true);

        const out = new Uint8Array(fileSize);
        out.set(header, 0);

        let offset = FILE_HEADER_SIZE;
        for (const part of this._parts) {
            out.set(part, offset);
            offset += part.length;
        }
        return out;
    }
}

/** `CHRS` 페이로드 */
export const encodeCharacters = (entries) =>
{
    const parts = [writeU32(entries.length)];
    for (const entry of entries) {
        parts.push(new Uint8Array([
            entry.kind,
            entry.exported ? 1 : 0,
            0,
            0
        ]));
        parts.push(writeU32(entry.bodyOffset));
        parts.push(writeU32(entry.bodySize));
    }
    return concatBytes(...parts);
};

/** 최소 MovieClip 본문 — 테스트용 */
export const encodeMovieClipBody = ({
    totalFrame = 0,
    dictionary = [],
    depthCount = 0,
    placeObjects = [],
    controller = [],
    placeMap = []
}) =>
{
    const parts = [
        writeU32(totalFrame),
        writeU32(dictionary.length)
    ];
    for (const entry of dictionary) {
        parts.push(
            writeU32(entry.characterId),
            writeU32(entry.startFrame),
            writeU32(entry.endFrame),
            writeI32(entry.clipDepth ?? -1)
        );
    }
    parts.push(writeU32(depthCount));
    parts.push(writeU32(placeObjects.length));
    for (const place of placeObjects) {
        parts.push(encodePlaceObject(place));
    }
    parts.push(writeU32(controller.length));
    for (const value of controller) {
        parts.push(writeI32(value));
    }
    parts.push(writeU32(placeMap.length));
    for (const value of placeMap) {
        parts.push(writeI32(value));
    }
    return concatBytes(...parts);
};

const PRESENT_MATRIX = 1 << 0;

const encodePlaceObject = (place) =>
{
    let present = 0;
    if (place.matrix?.length === 6) {
        present |= PRESENT_MATRIX;
    }
    const parts = [new Uint8Array([present, 0])];
    if (place.matrix?.length === 6) {
        for (const value of place.matrix) {
            parts.push(writeF32(value));
        }
    }
    return concatBytes(...parts);
};

const writeU32 = (value) =>
{
    const out = new Uint8Array(4);
    new DataView(out.buffer).setUint32(0, value >>> 0, true);
    return out;
};

const writeI32 = (value) =>
{
    const out = new Uint8Array(4);
    new DataView(out.buffer).setInt32(0, value | 0, true);
    return out;
};

const writeF32 = (value) =>
{
    const out = new Uint8Array(4);
    new DataView(out.buffer).setFloat32(0, value, true);
    return out;
};

const concatBytes = (...parts) =>
{
    const length = parts.reduce((sum, part) => sum + part.length, 0);
    const out = new Uint8Array(length);
    let offset = 0;
    for (const part of parts) {
        out.set(part, offset);
        offset += part.length;
    }
    return out;
};

/** 필수 청크만 갖춘 재생 가능한 최소 파일. */
export const minimalFile = (stage = { "width": 1280, "height": 720, "fps": 24 }) =>
{
    return new ZwfWriter(FILE_FLAGS.STREAMABLE)
        .pushRaw("META", new TextEncoder().encode(JSON.stringify({ "tool": "zukbox/0.1.0" })))
        .pushRaw("STAG", encodeStage(stage))
        .pushRaw("CHRS", new Uint8Array(4))
        .pushRaw("MCLP", new Uint8Array(0))
        .finish();
};

/** `IShapePublishJson` → SHAP 본문 */
export const encodeShapeBody = ({
    bounds,
    recodes = [],
    grid = null,
    inBitmap = false,
    bitmapId = null
}) =>
{
    let flags = 0;
    if (inBitmap) {
        flags |= 1 << 1;
    }
    if (grid) {
        flags |= 1 << 0;
    }
    if (bitmapId != null) {
        flags |= 1 << 2;
    }

    const parts = [
        writeF32(bounds.xMin),
        writeF32(bounds.xMax),
        writeF32(bounds.yMin),
        writeF32(bounds.yMax),
        new Uint8Array([flags, 0, 0, 0])
    ];
    if (grid) {
        parts.push(writeF32(grid.x), writeF32(grid.y), writeF32(grid.w), writeF32(grid.h));
    }
    if (bitmapId != null) {
        parts.push(writeU32(bitmapId));
    }
    const flat = recodes.filter((value) => typeof value === "number");
    parts.push(writeU32(flat.length));
    for (const value of flat) {
        parts.push(writeF32(value));
    }
    return concatBytes(...parts);
};

const detectBitmapEncoding = (data) =>
{
    if (data.length >= 8 && data[0] === 0x89 && data[1] === 0x50 && data[2] === 0x4e && data[3] === 0x47) {
        return 1;
    }
    if (data.length >= 12
        && data[0] === 0x52 && data[1] === 0x49 && data[2] === 0x46 && data[3] === 0x46
        && data[8] === 0x57 && data[9] === 0x45 && data[10] === 0x42 && data[11] === 0x50) {
        return 2;
    }
    return 0;
};

/** `IBitmapPublishJson` → BMAP 본문 */
export const encodeBitmapBody = ({ bounds, buffer = [] }) =>
{
    const data = Uint8Array.from(buffer, (value) => value & 0xff);
    return concatBytes(
        writeF32(bounds.xMin),
        writeF32(bounds.xMax),
        writeF32(bounds.yMin),
        writeF32(bounds.yMax),
        new Uint8Array([detectBitmapEncoding(data), 0, 0, 0]),
        writeU32(data.length),
        data
    );
};

const detectVideoEncoding = (data) =>
{
    if (data.length >= 12 && data[4] === 0x66 && data[5] === 0x74 && data[6] === 0x79 && data[7] === 0x70) {
        return 0;
    }
    if (data.length >= 4 && data[0] === 0x1a && data[1] === 0x45 && data[2] === 0xdf && data[3] === 0xa3) {
        return 1;
    }
    if (data.length >= 4 && data[0] === 0x4f && data[1] === 0x67 && data[2] === 0x67 && data[3] === 0x53) {
        return 2;
    }
    return 0;
};

/** `IVideoPublishJson` → VIDS 본문 */
export const encodeVideoBody = ({
    bounds,
    buffer = [],
    volume = 1,
    loop = false,
    autoPlay = false
}) =>
{
    const data = Uint8Array.from(buffer, (value) => value & 0xff);
    let flags = 0;
    if (loop) {
        flags |= 1;
    }
    if (autoPlay) {
        flags |= 2;
    }
    return concatBytes(
        writeF32(bounds.xMin),
        writeF32(bounds.xMax),
        writeF32(bounds.yMin),
        writeF32(bounds.yMax),
        writeF32(volume),
        new Uint8Array([flags, detectVideoEncoding(data), 0, 0]),
        writeU32(data.length),
        data
    );
};

/** CHRS/MCLP 가 채워진 테스트 파일 */
export const sampleTimelineFile = () =>
{
    const body = encodeMovieClipBody({
        "totalFrame": 1,
        "dictionary": [{
            "characterId": 1,
            "startFrame": 0,
            "endFrame": 1,
            "clipDepth": -1
        }],
        "depthCount": 1,
        "placeObjects": [{
            "matrix": [1, 0, 0, 1, 100, 50]
        }],
        "controller": [0],
        "placeMap": [0]
    });

    const characters = encodeCharacters([{
        "kind": 1,
        "exported": true,
        "bodyOffset": 0,
        "bodySize": body.length
    }]);

    return new ZwfWriter(FILE_FLAGS.STREAMABLE)
        .pushRaw("META", new TextEncoder().encode("{}"))
        .pushRaw("STAG", encodeStage({ "width": 640, "height": 480, "fps": 12, "rootCharacterId": 0 }))
        .pushRaw("CHRS", characters)
        .pushRaw("MCLP", body)
        .finish();
};

/** SHAP/BMAP/VIDS 가 포함된 테스트 파일 */
export const sampleAssetFile = () =>
{
    const shapeBody = encodeShapeBody({
        "bounds": { "xMin": 0, "xMax": 100, "yMin": 0, "yMax": 50 },
        "recodes": [1, 2, 3]
    });
    const bitmapBody = encodeBitmapBody({
        "bounds": { "xMin": 0, "xMax": 2, "yMin": 0, "yMax": 2 },
        "buffer": [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255]
    });
    const videoBody = encodeVideoBody({
        "bounds": { "xMin": 0, "xMax": 320, "yMin": 0, "yMax": 240 },
        "buffer": [0, 0, 0, 0x20, 0x66, 0x74, 0x79, 0x70],
        "volume": 0.8,
        "loop": true,
        "autoPlay": false
    });

    const characters = encodeCharacters([
        {
            "kind": 2,
            "exported": false,
            "bodyOffset": 0,
            "bodySize": shapeBody.length
        },
        {
            "kind": 3,
            "exported": false,
            "bodyOffset": 0,
            "bodySize": bitmapBody.length
        },
        {
            "kind": 4,
            "exported": false,
            "bodyOffset": 0,
            "bodySize": videoBody.length
        }
    ]);

    return new ZwfWriter(FILE_FLAGS.STREAMABLE)
        .pushRaw("META", new TextEncoder().encode("{}"))
        .pushRaw("STAG", encodeStage({ "width": 640, "height": 480, "fps": 12, "rootCharacterId": 0 }))
        .pushRaw("CHRS", characters)
        .pushRaw("SHAP", shapeBody)
        .pushRaw("BMAP", bitmapBody)
        .pushRaw("MCLP", new Uint8Array(0))
        .pushRaw("VIDS", videoBody)
        .finish();
};
