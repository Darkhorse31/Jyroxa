#!/usr/bin/env node
/**
 * Desktop UI E2E via WebView2 remote debugging (CDP).
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawn, execSync } from "node:child_process";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const desktopRoot = path.resolve(__dirname, "..");
const shotsDir =
  process.env.E2E_SHOTS_DIR ||
  "C:\\Users\\lenovo\\.jyroxa-tools\\reports\\e2e-shots";
const targetDir =
  process.env.CARGO_TARGET_DIR ||
  "C:\\Users\\lenovo\\.jyroxa-tools\\targets\\desktop";
const CDP_PORT = Number(process.env.E2E_CDP_PORT || 9333);

fs.mkdirSync(shotsDir, { recursive: true });

function findExe() {
  const candidates = [
    path.join(targetDir, "release", "jyroxa-desktop.exe"),
    path.join(targetDir, "release", "Jyroxa.exe"),
    path.join(
      desktopRoot,
      "src-tauri",
      "target",
      "release",
      "jyroxa-desktop.exe"
    ),
  ];
  for (const c of candidates) {
    if (fs.existsSync(c)) return c;
  }
  throw new Error(`Built exe not found (looked in ${candidates.join(", ")})`);
}

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

class CdpSession {
  constructor(wsUrl) {
    this.wsUrl = wsUrl;
    this.ws = null;
    this.nextId = 1;
    this.pending = new Map();
    this.consoleErrors = [];
    this.invokeErrors = [];
  }

  async connect() {
    this.ws = new WebSocket(this.wsUrl);
    await new Promise((resolve, reject) => {
      this.ws.addEventListener("open", resolve, { once: true });
      this.ws.addEventListener("error", reject, { once: true });
    });
    this.ws.addEventListener("message", (ev) => {
      const msg = JSON.parse(ev.data);
      if (msg.id && this.pending.has(msg.id)) {
        const { resolve, reject } = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        if (msg.error) reject(new Error(msg.error.message));
        else resolve(msg.result);
      }
      if (msg.method === "Console.messageAdded") {
        const entry = msg.params?.message;
        if (entry?.level === "error") this.consoleErrors.push(entry.text);
      }
      if (msg.method === "Runtime.consoleAPICalled") {
        const args = msg.params?.args || [];
        const text = args.map((a) => a.value ?? a.description ?? "").join(" ");
        if (msg.params?.type === "error") this.consoleErrors.push(text);
        if (/invoke|IPC|invalid args/i.test(text)) this.invokeErrors.push(text);
      }
    });
    await this.send("Runtime.enable");
    await this.send("Page.enable");
    await this.send("DOM.enable");
  }

  send(method, params = {}) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }

  async eval(expression) {
    const r = await this.send("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (r.exceptionDetails) {
      const ex = r.exceptionDetails.exception?.description;
      throw new Error(ex || r.exceptionDetails.text || "eval failed");
    }
    return r.result?.value;
  }

  async waitForSelector(selector, timeoutMs = 15_000) {
    const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
      const found = await this.eval(
        `!!document.querySelector(${JSON.stringify(selector)})`
      );
      if (found) return;
      await sleep(250);
    }
    throw new Error(`Timeout waiting for ${selector}`);
  }

  async clickSelector(selector) {
    await this.eval(`
      (() => {
        const el = document.querySelector(${JSON.stringify(selector)});
        if (!el) throw new Error('missing ${selector}');
        el.click();
      })()
    `);
  }

  async screenshot(name) {
    const r = await this.send("Page.captureScreenshot", { format: "png" });
    const out = path.join(shotsDir, `${name}.png`);
    fs.writeFileSync(out, Buffer.from(r.data, "base64"));
    return out;
  }

  close() {
    this.ws?.close();
  }
}

async function waitForCdp(timeoutMs = 120_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`http://127.0.0.1:${CDP_PORT}/json`);
      const list = await res.json();
      const page = list.find((t) => t.type === "page" || t.url?.includes("tauri"));
      if (page?.webSocketDebuggerUrl) return page.webSocketDebuggerUrl;
    } catch {
      /* retry */
    }
    await sleep(500);
  }
  throw new Error("CDP endpoint not available");
}

const steps = [];

function step(name, fn) {
  steps.push({ name, fn });
}

step("launch app", async () => {
  const exe = findExe();
  const dataRoot = path.join(shotsDir, "e2e-data");
  fs.mkdirSync(dataRoot, { recursive: true });
  try {
    execSync("taskkill /IM jyroxa-desktop.exe /F", { stdio: "ignore" });
  } catch {
    /* none running */
  }
  await sleep(2000);
  globalThis.__child = spawn(exe, [], {
    env: {
      ...process.env,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${CDP_PORT}`,
    },
    stdio: "ignore",
    detached: false,
  });
  globalThis.__wsUrl = await waitForCdp();
  globalThis.__cdp = new CdpSession(globalThis.__wsUrl);
  await globalThis.__cdp.connect();
  await globalThis.__cdp.waitForSelector("#btn-add", 30_000);
  await sleep(1000);
  await globalThis.__cdp.screenshot("01-boot");
});

step("add download", async () => {
  const cdp = globalThis.__cdp;
  fs.mkdirSync(path.join(shotsDir, "e2e-downloads"), { recursive: true });
  await cdp.clickSelector("#btn-add");
  await cdp.waitForSelector("#add-urls");
  await cdp.eval(`
    (() => {
      const u = document.getElementById('add-urls');
      const f = document.getElementById('add-filename');
      const d = document.getElementById('add-folder');
      if (!u || !f || !d) throw new Error('add dialog fields missing');
      u.value = 'https://proof.ovh.net/files/10Mb.dat';
      f.value = '10Mb.dat';
      d.value = ${JSON.stringify(path.join(shotsDir, "e2e-downloads"))};
      const btn = document.querySelector('#form-add button[type=submit]');
      if (!btn) throw new Error('submit button missing');
      btn.click();
    })()
  `);
  let listed = 0;
  for (let i = 0; i < 60; i++) {
    listed = await cdp.eval(`document.querySelectorAll('.dl-card').length`);
    if (listed > 0) break;
    const dlgOpen = await cdp.eval(
      `document.getElementById('dlg-add')?.open === true`
    );
    if (dlgOpen && i > 10) {
      await cdp.clickSelector("#form-add button[type=submit]");
    }
    await sleep(500);
  }
  if (listed === 0) {
    await cdp.send("Page.reload");
    await cdp.waitForSelector("#btn-add", 30_000);
    await sleep(2000);
    listed = await cdp.eval(`document.querySelectorAll('.dl-card').length`);
  }
  if (listed === 0) {
    const diag = await cdp.eval(`
      (async () => {
        const cards = document.querySelectorAll('.dl-card').length;
        let backend = -1;
        try {
          const rows = await window.__TAURI__.core.invoke('list_downloads');
          backend = rows?.length ?? 0;
        } catch (e) {
          backend = String(e);
        }
        return JSON.stringify({ cards, backend });
      })()
    `);
    throw new Error(`No download card appeared: ${diag}`);
  }
  await cdp.screenshot("02-queued");
});

step("pause and resume", async () => {
  const cdp = globalThis.__cdp;
  await cdp.waitForSelector(".dl-card .btn-pause");
  await cdp.eval(`
    (() => {
      const card = document.querySelector('.dl-card');
      const pause = card?.querySelector('.btn-pause');
      if (!pause) throw new Error('pause button missing');
      pause.click();
    })()
  `);
  await sleep(1500);
  await cdp.eval(`
    (() => {
      const card = document.querySelector('.dl-card');
      const resume = card?.querySelector('[data-slot="btn-resume"], .btn-resume');
      resume?.click();
    })()
  `);
  await sleep(1500);
  await cdp.screenshot("03-pause-resume");
});

step("theme toggle", async () => {
  const cdp = globalThis.__cdp;
  await cdp.waitForSelector("#btn-theme");
  await cdp.clickSelector("#btn-theme");
  await sleep(400);
  await cdp.screenshot("04-theme");
});

step("open link grabber", async () => {
  const cdp = globalThis.__cdp;
  await cdp.waitForSelector("#btn-grabber");
  await cdp.clickSelector("#btn-grabber");
  await cdp.waitForSelector("#grab-input", 10_000);
  await sleep(800);
  await cdp.screenshot("05-linkgrabber");
});

step("settings panel", async () => {
  const cdp = globalThis.__cdp;
  await cdp.waitForSelector("#btn-settings");
  await cdp.clickSelector("#btn-settings");
  await sleep(400);
  await sleep(800);
  await cdp.screenshot("06-settings");
});

async function main() {
  const failed = [];
  try {
    for (const s of steps) {
      process.stdout.write(`STEP ${s.name}... `);
      try {
        await s.fn();
        console.log("PASS");
      } catch (e) {
        console.log("FAIL");
        failed.push({ step: s.name, error: String(e) });
      }
    }
  } finally {
    const cdp = globalThis.__cdp;
    if (cdp) {
      if (cdp.consoleErrors.length) {
        failed.push({
          step: "console",
          error: cdp.consoleErrors.slice(0, 5).join("; "),
        });
      }
      if (cdp.invokeErrors.length) {
        failed.push({
          step: "invoke",
          error: cdp.invokeErrors.slice(0, 5).join("; "),
        });
      }
      cdp.close();
    }
  }
  if (globalThis.__child?.pid) {
    try {
      execSync(`taskkill /PID ${globalThis.__child.pid} /T /F`, {
        stdio: "ignore",
      });
    } catch {
      /* already exited */
    }
  }
  if (failed.length) {
    console.error("\nE2E failures:");
    for (const f of failed) console.error(`  - ${f.step}: ${f.error}`);
    process.exit(1);
  }
  console.log("\nE2E all steps passed.");
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
