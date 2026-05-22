#!/usr/bin/env node
/**
 * Build smoke test: load the production `dist/` bundle in a fake browser
 * environment and assert no JS runtime errors fire during module init.
 *
 * Reproduces the v1.4.x white-screen TDZ bug ("Cannot access 'ms' before
 * initialization") without needing Tauri / WebView2 / a real browser.
 *
 * Usage:
 *   npm run build && node scripts/smoke-test.mjs
 *
 * Exits 0 on clean load, 1 on any error. Designed to be plugged into CI as
 * the gate between `vite build` and `tauri build`.
 */

import { readFile, readdir } from "node:fs/promises";
import { resolve, dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { JSDOM, ResourceLoader, VirtualConsole } from "jsdom";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");
const distDir = resolve(root, "dist");

// Pin sensible globals before bringing up jsdom — gives the bundle a chance
// to import dependencies that read `window.crypto` etc at module top level.
const virtualConsole = new VirtualConsole();
let consoleErrors = [];
virtualConsole.on("jsdomError", (e) => {
  // jsdom emits TDZ-class errors here, not via the console listeners.
  consoleErrors.push({ kind: "jsdomError", err: e });
});
virtualConsole.on("error", (msg, ...args) => {
  consoleErrors.push({ kind: "console.error", msg, args });
});

// JSDOM's default resource loader only fetches http(s). Override to serve
// `./assets/...` out of dist/ via the file system.
class LocalLoader extends ResourceLoader {
  fetch(url, opts) {
    if (url.startsWith("about:")) return super.fetch(url, opts);
    // Tauri uses http://tauri.localhost/... — translate to dist file.
    let m = url.match(/^https?:\/\/[^/]+(\/.*)$/);
    if (m) {
      const rel = m[1].replace(/^\//, "");
      const path = resolve(distDir, rel);
      return readFile(path).then((buf) => buf);
    }
    return super.fetch(url, opts);
  }
}

const indexHtml = await readFile(join(distDir, "index.html"), "utf8");

// Patch the HTML to make module loading work: jsdom doesn't really execute
// `<script type="module">` from a file URL. We strip the type attr so the
// scripts run as classic — for the TDZ check that's fine because Rollup
// already concatenated everything.
const patchedHtml = indexHtml.replace(/type="module"/g, "");

const dom = new JSDOM(patchedHtml, {
  url: "http://tauri.localhost/",
  runScripts: "dangerously",
  resources: new LocalLoader(),
  pretendToBeVisual: true,
  virtualConsole,
});

// Capture runtime errors from inside the page too.
dom.window.addEventListener("error", (e) => {
  consoleErrors.push({
    kind: "window.error",
    msg: String(e.error?.stack ?? e.message),
  });
});
dom.window.addEventListener("unhandledrejection", (e) => {
  consoleErrors.push({
    kind: "unhandledrejection",
    msg: String(e.reason?.stack ?? e.reason),
  });
});

// Give the modules time to import + initialize. Most bundles finish in
// well under a second on a modern machine.
await new Promise((r) => setTimeout(r, 3000));

// We deliberately IGNORE app-level errors caused by Tauri runtime APIs
// (`invoke`, `listen`) being absent under jsdom. Those would create false
// positives. What we DO want to catch is bundle/init-time errors like
// TDZ, SyntaxError, "Cannot read properties of undefined (reading X)" from
// vendor circular deps, etc.
const FATAL_PATTERNS = [
  /Cannot access .+ before initialization/i, // TDZ — the v1.4.x white-screen bug
  /SyntaxError/,
  /Unexpected token/,
  /is not defined(?!:)/,
  /Maximum call stack/,
];

const fatal = consoleErrors.filter((e) => {
  const msg = String(e.err?.stack ?? e.err ?? e.msg ?? "");
  return FATAL_PATTERNS.some((p) => p.test(msg));
});

if (fatal.length > 0) {
  console.error("❌ Smoke test FAILED — bundle threw a fatal error:\n");
  for (const e of fatal) {
    console.error(`  [${e.kind}] ${e.err?.stack ?? e.err ?? e.msg ?? ""}`);
  }
  process.exit(1);
}

// Also verify common red flags in the HTML itself.
const flags = [];
if (/\scrossorigin(?=[ >])/.test(indexHtml)) {
  flags.push("HTML contains crossorigin attribute — Tauri 2 will reject");
}
if (/src="\/assets\//.test(indexHtml)) {
  flags.push("HTML uses absolute /assets/ paths — should be ./assets/");
}
const assets = await readdir(join(distDir, "assets"));
const vendorChunks = assets.filter((f) => f.startsWith("vendor-"));
if (vendorChunks.length > 0) {
  flags.push(
    `Bundle has vendor-* chunks (${vendorChunks.join(
      ", "
    )}) — manualChunks is known-broken in Tauri 2 webview, drop the splitting`
  );
}

if (flags.length > 0) {
  console.error("❌ Smoke test FAILED — HTML/asset layout red flags:");
  for (const f of flags) console.error("  - " + f);
  process.exit(1);
}

console.log("✓ Smoke test passed — bundle initializes cleanly");
console.log(`  HTML: ${indexHtml.length} bytes`);
console.log(`  Assets: ${assets.length} files`);
