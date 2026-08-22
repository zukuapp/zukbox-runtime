/**
 * ZWF evalFrame 결과 → Next2D render-queue 레이아웃 변환.
 *
 * Player 의 `CommandRenderUseCase` 가 읽는 형식:
 *   [bgColor u32] + (visible, type, …)*
 *
 * v0: 배경색 + Shape(recodes) 까지. Bitmap/Video 는 ImageBitmap 전달이 필요해
 *     zukbox 쪽 ZwfPlaybackService(Loader 경로)와 병행한다.
 */

/** Next2D `DisplayObjectUtil.$RENDERER_SHAPE_TYPE` */
export const RENDERER_SHAPE_TYPE = 0x01;

/** `0xRRGGBBAA` → CommandRenderUseCase 배경색 (`0xRRGGBB`) */
export const stageBgToRenderColor = (bgRgba) =>
    ((bgRgba >>> 24) & 0xff) << 16
    | ((bgRgba >>> 16) & 0xff) << 8
    | ((bgRgba >>> 8) & 0xff);

/**
 * @param {import("./zwf-loader.mjs").ZwfFile} file
 * @param {number} frame
 * @param {Map<number, { bounds: number[], recodes: Float32Array }>} [shapeByCharacterId]
 * @returns {Float32Array}
 */
export const buildFrameRenderQueue = (file, frame, shapeByCharacterId = new Map()) =>
{
    const items = file.evalFrame(file.stage.rootCharacterId, frame);
    const parts = [stageBgToRenderColor(file.stage.bgRgba)];

    for (const item of items) {
        const shape = shapeByCharacterId.get(item.characterId);
        if (!shape?.recodes?.length) {
            parts.push(0);
            continue;
        }

        const [xMin, yMin, xMax, yMax] = shape.bounds;
        const m = item.matrix;
        const c = item.colorTransform;

        // pushShapeBuffer 32 floats — ShapeGenerateRenderQueueUseCase 와 동일 순서
        parts.push(
            1, RENDERER_SHAPE_TYPE,
            m[0], m[1], m[2], m[3], m[4], m[5],
            c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7],
            xMin, yMin, xMax, yMax,
            xMin, yMin, xMax, yMax,
            0, 1, 0,
            item.characterId, 0,
            1, 1,
            0
        );

        // cache miss branch
        parts.push(0);
        parts.push(shape.recodes.length);
        for (let idx = 0; idx < shape.recodes.length; ++idx) {
            parts.push(shape.recodes[idx]);
        }
    }

    return new Float32Array(parts);
};
