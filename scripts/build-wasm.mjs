import { spawnSync } from "node:child_process";
import { mkdirSync, copyFileSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";

const root = fileURLToPath(new URL("../", import.meta.url));
const result = spawnSync("cargo", [
    "build", "--locked", "--release", "--target", "wasm32-unknown-unknown",
    "-p", "zwf-runtime"
], { cwd: root, stdio: "inherit", env: { ...process.env, CARGO_BUILD_JOBS: "1" } });
if (result.error || result.status !== 0) {
    throw new Error("WASM release build failed");
}
const source = new URL("../target/wasm32-unknown-unknown/release/zwf_runtime.wasm", import.meta.url);
const output = new URL("../dist/zwf_runtime.wasm", import.meta.url);
const bytes = readFileSync(source);
if (!WebAssembly.validate(bytes) || bytes.byteLength > 128 * 1024) {
    throw new Error("Invalid WASM or release size budget exceeded");
}
mkdirSync(new URL("../dist/", import.meta.url), { recursive: true });
copyFileSync(source, output);
console.log(JSON.stringify({ wasmBytes: bytes.byteLength,
    wasmSha256: createHash("sha256").update(bytes).digest("hex") }));
