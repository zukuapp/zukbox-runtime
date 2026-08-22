/**
 * ZWF 런타임 로더 — WASM 모듈을 감싸는 얇은 호스트 층.
 *
 * 여기서 하는 일은 세 가지뿐이다.
 *   1. `.wasm` 인스턴스화 (glue 코드 없음 — 순수 C ABI 를 직접 부른다)
 *   2. 파일 바이트를 WASM 선형 메모리로 옮기기
 *   3. 숫자 오류 코드를 사람이 읽을 수 있는 예외로 바꾸기
 *
 * 렌더링은 하지 않는다. 그건 Next2D Player 의 렌더러 워커 몫이다.
 */

/** `zwf_format::Error::code` 와 1:1로 대응한다. 값이 바뀌면 안 된다. */
const ERROR_MESSAGES = Object.freeze({
    [-1]: "파일이 32바이트 ZWF 헤더보다 짧습니다",
    [-2]: "ZWF 파일이 아닙니다 (매직 넘버 불일치)",
    [-3]: "헤더 CRC 불일치 — 파일이 손상되었습니다",
    [-4]: "지원하지 않는 포맷 버전입니다 — 런타임을 업데이트하세요",
    [-5]: "헤더 크기가 올바르지 않습니다",
    [-6]: "선언된 파일 크기가 실제 크기와 다릅니다",
    [-7]: "청크가 파일 끝을 넘어갑니다 — 잘린 파일입니다",
    [-8]: "청크 개수가 헤더 선언과 다릅니다",
    [-9]: "알 수 없는 압축 코덱입니다",
    [-10]: "무압축 청크의 크기 필드가 어긋납니다",
    [-11]: "압축 해제에 실패했습니다",
    [-12]: "이 런타임 빌드에 해당 코덱이 없습니다",
    [-13]: "필수 청크가 없습니다",
    [-14]: "청크 페이로드 형식이 잘못되었습니다",
    [-100]: "잘못된 인자입니다",
    [-101]: "유효하지 않은 핸들입니다",
    [-102]: "출력 버퍼가 부족합니다"
});

/** 재생을 시작할 수 없을 때 던진다. */
export class ZwfError extends Error
{
    /**
     * @param {number} code
     */
    constructor (code)
    {
        super(ERROR_MESSAGES[code] ?? `알 수 없는 ZWF 오류 (${code})`);
        this.name = "ZwfError";
        this.code = code;
    }
}

/** FourCC 를 담은 리틀엔디언 u32 를 문자열로 되돌린다. */
const fourCCToString = (value) =>
{
    return String.fromCharCode(
        value & 0xff,
        (value >>> 8) & 0xff,
        (value >>> 16) & 0xff,
        (value >>> 24) & 0xff
    );
};

/**
 * 열려 있는 `.zwf` 하나. 반드시 `close()` 로 닫아야 WASM 메모리가 회수된다.
 */
export class ZwfFile
{
    /**
     * @param {ZwfRuntime} runtime
     * @param {number} handle
     */
    constructor (runtime, handle)
    {
        this._runtime = runtime;
        this._handle = handle;
    }

    get closed ()
    {
        return this._handle < 0;
    }

    _exports ()
    {
        if (this.closed) {
            throw new ZwfError(-101);
        }
        return this._runtime.exports;
    }

    /** 스테이지 정보 — 캔버스를 만들기 위해 가장 먼저 필요한 값. */
    get stage ()
    {
        const exports = this._exports();
        const bgRgba = exports.zwf_stage_bg_rgba(this._handle);

        return {
            "width": exports.zwf_stage_width(this._handle),
            "height": exports.zwf_stage_height(this._handle),
            "fps": exports.zwf_stage_fps(this._handle),
            "bgRgba": bgRgba,
            // CSS 로 바로 쓸 수 있는 형태도 같이 준다.
            "bgColor": `#${(bgRgba >>> 8).toString(16).padStart(6, "0")}`,
            "bgAlpha": (bgRgba & 0xff) / 255,
            "rootCharacterId": exports.zwf_root_character_id(this._handle)
        };
    }

    /** 파일에 들어 있는 청크 FourCC 목록. 진단·기능 탐지용. */
    get chunkIds ()
    {
        const exports = this._exports();
        const count = exports.zwf_chunk_count(this._handle);
        if (count < 0) {
            throw new ZwfError(count);
        }

        const ids = new Array(count);
        for (let idx = 0; idx < count; ++idx) {
            ids[idx] = fourCCToString(exports.zwf_chunk_id_at(this._handle, idx));
        }
        return ids;
    }

    get flags ()
    {
        const bits = this._exports().zwf_file_flags(this._handle);
        if (bits < 0) {
            throw new ZwfError(bits);
        }
        return {
            "signed": (bits & 0b01) !== 0,
            "streamable": (bits & 0b10) !== 0
        };
    }

    get characterCount ()
    {
        const count = this._exports().zwf_character_count(this._handle);
        if (count < 0) {
            throw new ZwfError(count);
        }
        return count;
    }

    mclipTotalFrame (characterId = 0)
    {
        const total = this._exports().zwf_mclip_total_frame(this._handle, characterId);
        if (total < 0) {
            throw new ZwfError(total);
        }
        return total;
    }

    /**
     * MovieClip 타임라인 프레임을 평가해 렌더 목록을 만든다.
     *
     * @param {number} [characterId] 평가할 MovieClip characterId
     * @param {number} [frame=0] 프레임 번호
     * @returns {Array<{
     *   depth: number,
     *   characterId: number,
     *   matrix: Float32Array,
     *   colorTransform: Float32Array,
     *   blendMode: number
     * }>}
     */
    evalFrame (characterId = this.stage.rootCharacterId, frame = 0)
    {
        const exports = this._exports();
        const count = exports.zwf_eval_frame(this._handle, characterId, frame, 0, 0);
        if (count < 0) {
            throw new ZwfError(count);
        }
        if (count === 0) {
            return [];
        }

        const stride = exports.zwf_render_item_stride();
        const floatCount = count * stride;
        const byteLen = floatCount * 4;
        const ptr = exports.zwf_alloc(byteLen);
        if (ptr === 0) {
            throw new ZwfError(-100);
        }

        try {
            const written = exports.zwf_eval_frame(
                this._handle,
                characterId,
                frame,
                ptr,
                floatCount
            );
            if (written < 0) {
                throw new ZwfError(written);
            }

            const view = new Float32Array(exports.memory.buffer, ptr, floatCount);
            const items = new Array(written);
            for (let idx = 0; idx < written; ++idx) {
                const base = idx * stride;
                items[idx] = {
                    "depth": view[base],
                    "characterId": view[base + 1],
                    "matrix": view.subarray(base + 2, base + 8),
                    "colorTransform": view.subarray(base + 8, base + 16),
                    "blendMode": view[base + 16]
                };
            }
            return items;
        } finally {
            exports.zwf_free(ptr, byteLen);
        }
    }

    close ()
    {
        if (this.closed) {
            return;
        }
        this._runtime.exports.zwf_close(this._handle);
        this._handle = -1;
    }
}

/**
 * 인스턴스화된 WASM 런타임. 페이지당 하나면 충분하다.
 */
export class ZwfRuntime
{
    /**
     * @param {WebAssembly.Instance} instance
     */
    constructor (instance)
    {
        this.exports = instance.exports;
    }

    /**
     * `.wasm` 을 받아 런타임을 만든다.
     *
     * `Response` 를 넘기면 `instantiateStreaming` 으로 다운로드와 컴파일이
     * 겹쳐 실행된다 — 첫 재생까지의 시간이 줄어드니 되도록 이 경로를 쓴다.
     *
     * @param {Response | Promise<Response> | BufferSource} source
     * @returns {Promise<ZwfRuntime>}
     */
    static async instantiate (source)
    {
        const resolved = await source;

        const result = resolved instanceof Response || typeof resolved?.arrayBuffer === "function"
            ? await WebAssembly.instantiateStreaming(resolved, {})
            : await WebAssembly.instantiate(resolved, {});

        return new ZwfRuntime(result.instance);
    }

    /** 이 모듈이 구현하는 명세 버전. */
    get specVersion ()
    {
        const packed = this.exports.zwf_spec_version();
        return {
            "major": packed >>> 16,
            "minor": packed & 0xffff
        };
    }

    /** 아직 닫지 않은 파일 수. 누수 점검용. */
    get openCount ()
    {
        return this.exports.zwf_open_count();
    }

    /**
     * `.zwf` 바이트를 파싱한다.
     *
     * 성공하면 버퍼 소유권은 WASM 으로 넘어간다. 실패하면 WASM 쪽에서
     * 해제되므로 어느 경로에서도 누수가 없다.
     *
     * @param {ArrayBuffer | Uint8Array} bytes
     * @returns {ZwfFile}
     */
    open (bytes)
    {
        const data = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);

        const ptr = this.exports.zwf_alloc(data.length);
        if (ptr === 0) {
            throw new ZwfError(-100);
        }

        // memory 뷰는 WASM 이 힙을 키우면 무효화된다. alloc 직후에 잡는다.
        new Uint8Array(this.exports.memory.buffer, ptr, data.length).set(data);

        const handle = this.exports.zwf_open(ptr, data.length);
        if (handle < 0) {
            throw new ZwfError(handle);
        }

        return new ZwfFile(this, handle);
    }
}
