/**
 * ZWF 재생기 — WASM 타임라인 평가 + Next2D 렌더러 워커.
 *
 * ```js
 * import { ZwfPlayer } from "@zukbox/runtime/player";
 * const player = await ZwfPlayer.open("/runtime/zwf_runtime.wasm", bytes, canvas);
 * player.play();
 * ```
 */

import { ZwfRuntime } from "./zwf-loader.mjs";
import { buildFrameRenderQueue } from "./zwf-render-queue.mjs";

export { buildFrameRenderQueue, RENDERER_SHAPE_TYPE } from "./zwf-render-queue.mjs";

export class ZwfPlayer
{
    /**
     * @param {import("./zwf-loader.mjs").ZwfRuntime} runtime
     * @param {import("./zwf-loader.mjs").ZwfFile} file
     * @param {Worker} rendererWorker
     * @param {HTMLCanvasElement} canvas
     */
    constructor (runtime, file, rendererWorker, canvas)
    {
        this._runtime = runtime;
        this._file = file;
        this._worker = rendererWorker;
        this._canvas = canvas;
        this._frame = 0;
        this._playing = false;
        this._raf = 0;
        this._lastTick = 0;
        this._shapeByCharacterId = new Map();
    }

    /**
     * @param {string | URL | Response | BufferSource} wasmSource
     * @param {Uint8Array} zwfBytes
     * @param {HTMLCanvasElement} canvas
     * @param {Worker} rendererWorker — Next2D `@next2d/renderer` worker
     */
    static async open (wasmSource, zwfBytes, canvas, rendererWorker)
    {
        const wasmBytes = typeof wasmSource === "string"
            ? await fetch(wasmSource).then((response) => response.arrayBuffer())
            : wasmSource;

        const runtime = await ZwfRuntime.instantiate(wasmBytes);
        const file = runtime.open(zwfBytes);

        if (!canvas.dataset.zwfPlayerBooted) {
            const offscreen = canvas.transferControlToOffscreen();
            rendererWorker.postMessage({
                "command": "initialize",
                "canvas": offscreen,
                "devicePixelRatio": window.devicePixelRatio
            }, [offscreen]);
            canvas.dataset.zwfPlayerBooted = "1";
        }

        rendererWorker.postMessage({
            "command": "resize",
            "width": file.stage.width,
            "height": file.stage.height,
            "devicePixelRatio": window.devicePixelRatio
        });

        return new ZwfPlayer(runtime, file, rendererWorker, canvas);
    }

    /** Shape recodes 를 characterId 에 바인딩 (SHAP 디코딩 결과) */
    registerShape (characterId, bounds, recodes)
    {
        this._shapeByCharacterId.set(characterId, {
            "bounds": bounds,
            "recodes": recodes instanceof Float32Array ? recodes : new Float32Array(recodes)
        });
    }

    get totalFrames ()
    {
        return this._file.mclipTotalFrame(this._file.stage.rootCharacterId);
    }

    get fps ()
    {
        return this._file.stage.fps;
    }

    renderFrame (frame = this._frame)
    {
        const queue = buildFrameRenderQueue(this._file, frame, this._shapeByCharacterId);
        this._worker.postMessage({
            "command": "render",
            "buffer": queue,
            "length": queue.length,
            "imageBitmaps": null
        }, [queue.buffer]);
        this._frame = frame;
    }

    play ()
    {
        if (this._playing) {
            return;
        }
        this._playing = true;
        this._lastTick = performance.now();
        const frameMs = 1000 / (this._file.stage.fps || 24);

        const tick = (now) =>
        {
            if (!this._playing) {
                return;
            }
            if (now - this._lastTick >= frameMs) {
                this._lastTick = now;
                const total = this.totalFrames;
                if (total > 0) {
                    this.renderFrame(this._frame % total);
                    this._frame += 1;
                }
            }
            this._raf = requestAnimationFrame(tick);
        };

        this._raf = requestAnimationFrame(tick);
    }

    stop ()
    {
        this._playing = false;
        if (this._raf) {
            cancelAnimationFrame(this._raf);
            this._raf = 0;
        }
    }

    close ()
    {
        this.stop();
        this._file.close();
    }
}
