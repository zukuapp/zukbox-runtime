import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { createServer } from "node:http";
import { createRequire } from "node:module";

const root = fileURLToPath(new URL("../", import.meta.url));
const temp = mkdtempSync(join(tmpdir(), "zukbox-runtime-consumer-"));
const consumer = join(temp, "consumer");
const run = (args, cwd) => {
    const r = spawnSync("npm", args, { cwd, encoding: "utf8", timeout: 180000, maxBuffer: 2 * 1024 * 1024 });
    if (r.status !== 0) throw new Error(`Packed consumer command failed (${r.status})`);
    return r.stdout;
};
let server, browser;
try {
    run(["pack", "--json", "--pack-destination", temp], root);
    const archive = join(temp, readdirSync(temp).find(name => name.endsWith(".tgz")));
    mkdirSync(consumer);
    writeFileSync(join(consumer, "package.json"), JSON.stringify({ name: "zukbox-independent-consumer", private: true, type: "module" }));
    run(["install", "--ignore-scripts", "--no-audit", "--no-fund", archive], consumer);
    const nodeSource = `import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {ZwfRuntime,wasmUrl,ZwfError} from '@zukbox/runtime';
import {sampleTimelineFile} from '@zukbox/runtime/writer';
import {ZwfPlayer} from '@zukbox/runtime/player';
import {buildFrameRenderQueue} from '@zukbox/runtime/render-queue';
const binary=await readFile(new URL(import.meta.resolve('@zukbox/runtime/wasm')));
assert.ok(WebAssembly.validate(binary));
const runtime=await ZwfRuntime.instantiate();
const file=runtime.open(sampleTimelineFile());
const items=file.evalFrame(); const queue=buildFrameRenderQueue(file,0);
assert.equal(file.stage.width,640);assert.equal(queue.length,2);
const p=runtime.exports.zwf_alloc(68);new Uint8Array(runtime.exports.memory.buffer,p,68).fill(0);
runtime.exports.zwf_free(p,68);runtime.exports.memory.grow(1);file.close();
assert.deepEqual([...items[0].matrix],[1,0,0,1,100,50]);assert.equal(runtime.openCount,0);
const v2=new Uint8Array(64);v2.set(new TextEncoder().encode('ZWF2'));
assert.throws(()=>runtime.open(v2),e=>e instanceof ZwfError&&e.code===-2);
assert.equal(typeof ZwfPlayer.open,'function');assert.ok(wasmUrl.startsWith('file:'));
console.log(JSON.stringify({pass:true,defaultAsset:true,allExports:true,ownedFrames:true,zwf2Rejected:true,node:process.version}));`;
    writeFileSync(join(consumer, "node-test.mjs"), nodeSource);
    const nodeRun = spawnSync(process.execPath, ["node-test.mjs"], { cwd: consumer, encoding: "utf8", timeout: 30000 });
    assert.equal(nodeRun.status, 0, nodeRun.stderr);
    const proof = { node: JSON.parse(nodeRun.stdout), artifactSha256: createHash("sha256").update(readFileSync(archive)).digest("hex"),
        browser: { skipped: true, reason: "Set ZUKU_RUNTIME_PLAYWRIGHT_MODULE and ZUKU_RUNTIME_BROWSER_EXECUTABLE for actual Chrome." },
        installation: "fresh tarball install; no checkout imports; no install scripts" };
    if (process.env.ZUKU_RUNTIME_PLAYWRIGHT_MODULE && process.env.ZUKU_RUNTIME_BROWSER_EXECUTABLE) {
        const prefix = "/node_modules/@zukbox/runtime";
        writeFileSync(join(consumer, "browser-test.mjs"), `
import {ZwfRuntime,ZwfError} from '${prefix}/js/zwf-loader.mjs';
import {sampleTimelineFile} from '${prefix}/js/zwf-writer.mjs';
import {ZwfPlayer} from '${prefix}/js/zwf-player.mjs';
import {buildFrameRenderQueue} from '${prefix}/js/zwf-render-queue.mjs';
const check=(value)=>{if(!value)throw Error('Installed browser runtime check failed');};
const runtime=await ZwfRuntime.instantiate();const file=runtime.open(sampleTimelineFile());
const items=file.evalFrame();check(file.stage.width===640&&file.stage.height===480);
check(buildFrameRenderQueue(file,0).length===2);
const ptr=runtime.exports.zwf_alloc(68);new Uint8Array(runtime.exports.memory.buffer,ptr,68).fill(0);
runtime.exports.zwf_free(ptr,68);runtime.exports.memory.grow(1);file.close();
check(items[0].matrix.join(',')==='1,0,0,1,100,50'&&runtime.openCount===0);
const bad=new Uint8Array(64);bad.set(new TextEncoder().encode('ZWF2'));let rejected=false;
try{runtime.open(bad);}catch(e){rejected=e instanceof ZwfError&&e.code===-2;}check(rejected);
check(typeof ZwfPlayer.open==='function');
const fallback=await ZwfRuntime.instantiate(fetch('/wasm-octet'));
check(fallback.specVersion.minor===1);
window.runtimeProof={pass:true,checks:{defaultModuleRelativeWasm:true,actualWasmParse:true,allEntrypoints:true,ownedFrameMemory:true,zwf2Rejected:true,mimeFallback:true},openCount:runtime.openCount};
`);
        const requests = [];
        server = createServer((req, res) => {
            const path = new URL(req.url, "http://localhost").pathname;
            requests.push(path);
            res.setHeader("Content-Security-Policy", "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; base-uri 'none'; object-src 'none'");
            if (path === "/") { res.setHeader("Content-Type", "text/html"); res.end('<!doctype html><script type="module" src="/browser-test.mjs"></script>'); return; }
            if (path === "/favicon.ico") { res.writeHead(204); res.end(); return; }
            const local = path === "/wasm-octet" ? join(consumer, "node_modules/@zukbox/runtime/dist/zwf_runtime.wasm") : resolve(consumer, "." + path);
            if (!local.startsWith(consumer + sep)) { res.writeHead(404); res.end(); return; }
            try {
                res.setHeader("Content-Type", path === "/wasm-octet" ? "application/octet-stream" : path.endsWith(".wasm") ? "application/wasm" : "text/javascript");
                res.end(readFileSync(local));
            } catch { res.writeHead(404); res.end(); }
        });
        await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
        const { chromium } = createRequire(import.meta.url)(process.env.ZUKU_RUNTIME_PLAYWRIGHT_MODULE);
        browser = await chromium.launch({ executablePath: process.env.ZUKU_RUNTIME_BROWSER_EXECUTABLE, args: ["--no-sandbox"] });
        const page = await browser.newPage();
        const errors = [], external = [];
        page.on("pageerror", e => errors.push(e.message));
        page.on("request", req => { if (!req.url().startsWith(`http://127.0.0.1:${server.address().port}/`)) external.push(req.url()); });
        await page.goto(`http://127.0.0.1:${server.address().port}/`);
        await page.waitForFunction(() => window.runtimeProof?.pass, null, { timeout: 30000 });
        const actual = await page.evaluate(() => window.runtimeProof);
        assert.equal(errors.length, 0); assert.equal(external.length, 0);
        assert.ok(requests.includes(prefix + "/dist/zwf_runtime.wasm"));
        proof.browser = { ...actual, pageErrors: errors, externalRequests: external, strictCsp: true, javascriptEvalAllowed: false,
            wasmOnlyCompilationAllowed: true, actualDefaultWasmRequest: true };
    }
    if (process.env.ZUKU_RUNTIME_PROOF_PATH) writeFileSync(process.env.ZUKU_RUNTIME_PROOF_PATH, JSON.stringify(proof, null, 2) + "\n");
    console.log(JSON.stringify(proof));
} finally {
    if (browser) await browser.close();
    if (server) await new Promise(resolve => server.close(resolve));
    rmSync(temp, { recursive: true, force: true });
}
