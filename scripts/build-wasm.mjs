import { spawnSync } from "node:child_process";
import { mkdirSync, copyFileSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { homedir } from "node:os";

const root = fileURLToPath(new URL("../", import.meta.url));
const inheritedFlags = process.env.CARGO_ENCODED_RUSTFLAGS
    ? process.env.CARGO_ENCODED_RUSTFLAGS.split("\x1f")
    : (process.env.RUSTFLAGS || "").trim().split(/\s+/).filter(Boolean);
const flags = [...inheritedFlags,
    `--remap-path-prefix=${root.replace(/\/$/, "")}=zuku-runtime`,
    `--remap-path-prefix=${homedir()}=build-home`,
    "--remap-path-prefix=/rustc=rustc"
];
const result = spawnSync("cargo", [
    "build", "--locked", "--release", "--target", "wasm32-unknown-unknown",
    "-p", "zwf-runtime"
], { cwd: root, stdio: "inherit", env: { ...process.env,
    CARGO_BUILD_JOBS: "1", CARGO_ENCODED_RUSTFLAGS: flags.join("\x1f") } });
if (result.error || result.status !== 0) {
    throw new Error("WASM release build failed");
}
const source = new URL("../target/wasm32-unknown-unknown/release/zwf_runtime.wasm", import.meta.url);
const output = new URL("../dist/zwf_runtime.wasm", import.meta.url);
const bytes = readFileSync(source);
if (!WebAssembly.validate(bytes) || bytes.byteLength > 128 * 1024) {
    throw new Error("Invalid WASM or release size budget exceeded");
}
// Standard library /rustc identifiers may survive precompiled Rust metadata.
// Host home/workspace paths must not be embedded in the published binary.
if (/\/(?:root|home|Users|srv|volume[0-9]+)\//.test(bytes.toString("latin1"))) {
    throw new Error("WASM release contains a private build-host path");
}
mkdirSync(new URL("../dist/", import.meta.url), { recursive: true });
copyFileSync(source, output);
console.error(JSON.stringify({ wasmBytes: bytes.byteLength,
    wasmSha256: createHash("sha256").update(bytes).digest("hex") }));
