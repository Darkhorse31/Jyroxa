#!/usr/bin/env node
/**
 * Desktop E2E over WebView2 remote debugging (CDP), no dependencies.
 *
 * Drives the real UI of a built or installed Jyroxa and checks every effect
 * outside the page: the queue (through the app's own commands), files on disk,
 * Explorer windows and the opened file's app.
 *
 *   E2E_EXE=<path to Jyroxa.exe>   (default: the installed copy, then the build)
 *   E2E_SHOTS_DIR=<dir>            screenshots + work dir
 *
 * Needs network access to proof.ovh.net and www.gnu.org.
 */
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawn, execFileSync } from "node:child_process";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const desktopRoot = path.resolve(__dirname, "..");
const shotsDir = process.env.E2E_SHOTS_DIR || path.join(os.tmpdir(), "jyroxa-e2e");
const workDir = path.join(shotsDir, "downloads");
const CDP_PORT = Number(process.env.E2E_CDP_PORT || 9333);

const SMALL_URL = "https://proof.ovh.net/files/1Mb.dat";
const BIG_URL = "https://proof.ovh.net/files/100Mb.dat";
const TEXT_URL = "https://www.gnu.org/licenses/gpl-3.0.txt";

fs.rmSync(workDir, { recursive: true, force: true });
fs.mkdirSync(workDir, { recursive: true });

function findExe() {
  const target = process.env.CARGO_TARGET_DIR || path.join(desktopRoot, "..", "..", "target");
  const candidates = [
    process.env.E2E_EXE,
    path.join(process.env.LOCALAPPDATA || "", "Jyroxa", "Jyroxa.exe"),
    path.join(process.env.ProgramFiles || "", "Jyroxa", "Jyroxa.exe"),
    path.join(target, "release", "jyroxa-desktop.exe"),
  ].filter(Boolean);
  for (const c of candidates) if (fs.existsSync(c)) return c;
  throw new Error(`Jyroxa exe not found (looked in ${candidates.join(", ")})`);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function ps(script) {
  return execFileSync("powershell", ["-NoProfile", "-Command", script], { encoding: "utf8" }).trim();
}

class Cdp {
  constructor(url) {
    this.url = url;
    this.id = 1;
    this.pending = new Map();
    this.errors = [];
  }
  async connect() {
    this.ws = new WebSocket(this.url);
    await new Promise((res, rej) => {
      this.ws.addEventListener("open", res, { once: true });
      this.ws.addEventListener("error", rej, { once: true });
    });
    this.ws.addEventListener("message", (ev) => {
      const msg = JSON.parse(ev.data);
      const p = msg.id && this.pending.get(msg.id);
      if (p) {
        this.pending.delete(msg.id);
        msg.error ? p.rej(new Error(msg.error.message)) : p.res(msg.result);
      }
      if (msg.method === "Runtime.exceptionThrown") {
        this.errors.push(msg.params.exceptionDetails?.exception?.description || msg.params.exceptionDetails?.text);
      }
      if (msg.method === "Runtime.consoleAPICalled" && msg.params.type === "error") {
        this.errors.push(msg.params.args.map((a) => a.value ?? a.description ?? "").join(" "));
      }
    });
    await this.send("Runtime.enable");
    await this.send("Page.enable");
  }
  send(method, params = {}) {
    const id = this.id++;
    return new Promise((res, rej) => {
      this.pending.set(id, { res, rej });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }
  async eval(expr) {
    const r = await this.send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true });
    if (r.exceptionDetails) {
      const ex = r.exceptionDetails.exception;
      throw new Error(ex?.description || (ex?.value !== undefined ? String(ex.value) : r.exceptionDetails.text));
    }
    return r.result?.value;
  }
  invoke(cmd, args = {}) {
    return this.eval(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`);
  }
  click(selector) {
    return this.eval(`(() => { const el = document.querySelector(${JSON.stringify(selector)});
      if (!el) throw new Error("missing element ${selector.replace(/"/g, "'")}"); el.click(); return true; })()`);
  }
  async shot(name) {
    const r = await this.send("Page.captureScreenshot", { format: "png" });
    fs.writeFileSync(path.join(shotsDir, `${name}.png`), Buffer.from(r.data, "base64"));
  }
}

async function waitFor(desc, fn, timeoutMs = 60_000, everyMs = 300) {
  const end = Date.now() + timeoutMs;
  let last;
  while (Date.now() < end) {
    try {
      last = await fn();
      if (last) return last;
    } catch (e) {
      last = e;
    }
    await sleep(everyMs);
  }
  throw new Error(`timed out waiting for ${desc} (last: ${last instanceof Error ? last.message : JSON.stringify(last)})`);
}

let cdp;
let child;
const list = () => cdp.invoke("list_downloads");
const item = async (id) => (await list()).find((d) => d.id === id);
const done = (s) => s === "completed" || s === "done";

async function waitStatus(id, pred, desc, timeoutMs = 90_000) {
  return waitFor(`#${id} ${desc}`, async () => {
    const d = await item(id);
    return d && pred(d.status) ? d : false;
  }, timeoutMs);
}

async function addViaInvoke(url, filename) {
  return cdp.invoke("add_download", {
    args: { urls: [url], output_dir: workDir, filename, connections: 4, connections_auto: true, sha256: null, md5: null },
  });
}

async function cardAction(id, selector) {
  // Click like a user: only once the card has rendered the button as usable.
  const sel = `.dl-card[data-id="${id}"] ${selector}`;
  await waitFor(`${sel} enabled`, () => cdp.eval(`(() => { const b = document.querySelector(${JSON.stringify(sel)});
    return !!b && !b.disabled && !b.classList.contains("hidden"); })()`), 15_000);
  await cdp.click(sel);
}

async function contextMenu(id, action) {
  await waitFor(`card #${id}`, () => cdp.eval(`!!document.querySelector('.dl-card[data-id="${id}"]')`), 10_000);
  await cdp.eval(`(() => { const c = document.querySelector('.dl-card[data-id="${id}"]');
    const r = c.getBoundingClientRect();
    c.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: r.left + 40, clientY: r.top + 20 }));
    return true; })()`);
  await sleep(200);
  await cdp.click(`#context-menu [data-action="${action}"]`);
}

async function removeDialog(button, shot) {
  await waitFor("remove dialog", () => cdp.eval(`document.getElementById("dlg-remove").open`), 5_000);
  if (shot) await cdp.shot(shot);
  await cdp.click(`#${button}`);
  await waitFor("remove dialog to close", () => cdp.eval(`!document.getElementById("dlg-remove").open`), 5_000);
}

function explorerWindows() {
  const out = ps(`(New-Object -ComObject Shell.Application).Windows() | % { try { $_.Document.Folder.Self.Path } catch {} }`);
  return out ? out.split(/\r?\n/).map((s) => s.trim()).filter(Boolean) : [];
}

function closeExplorerWindow(dir) {
  ps(`(New-Object -ComObject Shell.Application).Windows() | ? { try { $_.Document.Folder.Self.Path -eq '${dir.replace(/'/g, "''")}' } catch { $false } } | % { $_.Quit() }`);
}

const steps = [];
const step = (name, fn) => steps.push({ name, fn });
const ids = {};

step("launch installed app", async () => {
  const exe = findExe();
  console.log(`  exe: ${exe}`);
  child = spawn(exe, [], {
    env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${CDP_PORT}` },
    stdio: "ignore",
  });
  const ws = await waitFor("CDP", async () => {
    const targets = await (await fetch(`http://127.0.0.1:${CDP_PORT}/json`)).json();
    return targets.find((t) => t.type === "page")?.webSocketDebuggerUrl;
  }, 60_000, 500);
  cdp = new Cdp(ws);
  await cdp.connect();
  await waitFor("Tauri IPC", () => cdp.eval(`!!window.__TAURI_INTERNALS__ && !!document.getElementById("btn-add")`), 30_000);
  await sleep(1500);
  await cdp.shot("01-boot");
});

step("every read-only command accepts the frontend's arguments", async () => {
  const s = await cdp.invoke("get_settings");
  if (!s || typeof s.download_dir !== "string") throw new Error("get_settings returned no download_dir");
  await list();
  await cdp.invoke("default_category_dirs");
  const p = await cdp.invoke("suggest_save_path", { dir: workDir, name: "x.bin" });
  if (!String(p).includes("x")) throw new Error(`suggest_save_path: ${p}`);
  const probe = await cdp.invoke("probe_url", { url: SMALL_URL });
  if (probe.size !== 1048576 && probe.total !== 1048576 && !JSON.stringify(probe).includes("1048576")) {
    throw new Error(`probe_url size wrong: ${JSON.stringify(probe)}`);
  }
  await cdp.invoke("probe_links", { urls: [SMALL_URL] });
});

step("settings save round-trips", async () => {
  const before = await cdp.invoke("get_settings");
  const changed = { ...before, max_concurrent: before.max_concurrent === 4 ? 5 : 4 };
  await cdp.invoke("set_settings", { settings: changed });
  const after = await cdp.invoke("get_settings");
  await cdp.invoke("set_settings", { settings: before });
  if (after.max_concurrent !== changed.max_concurrent) throw new Error("set_settings did not persist");
});

step("theme toggle changes the theme", async () => {
  const before = await cdp.eval(`document.body.getAttribute("data-theme")`);
  await cdp.click("#btn-theme");
  await sleep(300);
  const after = await cdp.eval(`document.body.getAttribute("data-theme")`);
  await cdp.shot("02-theme");
  await cdp.click("#btn-theme");
  if (before === after) throw new Error(`theme stayed ${before}`);
});

step("add download through the Add dialog and complete it", async () => {
  await cdp.click("#btn-add");
  await waitFor("add dialog", () => cdp.eval(`document.getElementById("dlg-add").open`), 5_000);
  await cdp.eval(`(() => {
    const set = (id, v) => { const el = document.getElementById(id); el.value = v;
      el.dispatchEvent(new Event("input", { bubbles: true })); el.dispatchEvent(new Event("change", { bubbles: true })); };
    set("add-urls", ${JSON.stringify(SMALL_URL)});
    set("add-folder", ${JSON.stringify(workDir)});
    return true; })()`);
  await sleep(1500);
  await cdp.eval(`(() => { const el = document.getElementById("add-filename"); el.value = "small.dat";
    el.dispatchEvent(new Event("input", { bubbles: true })); return true; })()`);
  await cdp.shot("03-add-dialog");
  const before = new Set((await list()).map((d) => d.id));
  await cdp.click("#form-add button[type=submit]");
  const added = await waitFor("new queue item", async () => (await list()).find((d) => !before.has(d.id)), 15_000);
  ids.small = added.id;
  const d = await waitStatus(added.id, done, "completed");
  if (!fs.existsSync(d.output) || fs.statSync(d.output).size !== 1048576) {
    throw new Error(`file missing or wrong size: ${d.output}`);
  }
  ids.smallPath = d.output;
  await cdp.shot("04-completed");
});

step("pause, resume and cancel from the card buttons", async () => {
  const id = await addViaInvoke(BIG_URL, "big.dat");
  ids.big = id;
  await waitStatus(id, (s) => s === "downloading" || s === "running", "downloading", 30_000);
  await cardAction(id, ".btn-pause");
  await waitStatus(id, (s) => s === "paused", "paused", 15_000);
  await cdp.shot("05-paused");
  await cardAction(id, ".btn-resume");
  await waitStatus(id, (s) => s === "downloading" || s === "running" || done(s), "resumed", 30_000);
  await cardAction(id, ".btn-cancel");
  await waitStatus(id, (s) => s === "cancelled" || s === "canceled", "cancelled", 30_000);
  await cdp.shot("06-cancelled");
});

step("open folder selects the file in Explorer", async () => {
  const dir = path.dirname(ids.smallPath);
  closeExplorerWindow(dir);
  await cardAction(ids.small, ".btn-open-folder");
  await waitFor(`Explorer window on ${dir}`, () => explorerWindows().some((w) => w.toLowerCase() === dir.toLowerCase()), 15_000, 700);
  closeExplorerWindow(dir);
});

step("open folder on a deleted file opens its folder", async () => {
  const gone = path.join(workDir, "deleted-before-open.bin");
  closeExplorerWindow(workDir);
  await cdp.invoke("open_folder_cmd", { path: gone });
  await waitFor(`Explorer window on ${workDir}`, () => explorerWindows().some((w) => w.toLowerCase() === workDir.toLowerCase()), 15_000, 700);
  closeExplorerWindow(workDir);
});

step("open file launches its default app", async () => {
  const id = await addViaInvoke(TEXT_URL, "gpl-3.0.txt");
  ids.text = id;
  await waitStatus(id, done, "completed");
  const before = new Set(ps(`(Get-Process notepad -ErrorAction SilentlyContinue).Id -join ','`).split(",").filter(Boolean));
  await cardAction(id, ".btn-open-file");
  const pid = await waitFor("notepad for gpl-3.0.txt", () => {
    const now = ps(`(Get-Process notepad -ErrorAction SilentlyContinue).Id -join ','`).split(",").filter(Boolean);
    return now.find((p) => !before.has(p));
  }, 15_000, 700);
  ps(`Stop-Process -Id ${pid} -Force -ErrorAction SilentlyContinue`);
  let missing = null;
  try { await cdp.invoke("open_file", { path: path.join(workDir, "nope.txt") }); } catch (e) { missing = String(e); }
  if (!missing || !/not found/i.test(missing)) throw new Error(`open_file on a missing file should fail clearly, got ${missing}`);
});

step("copy URL writes the clipboard", async () => {
  await cdp.invoke("plugin:clipboard-manager|write_text", { text: "jyroxa-e2e-sentinel" });
  await contextMenu(ids.small, "copy-url");
  const text = await waitFor("clipboard to change", async () => {
    const t = await cdp.invoke("plugin:clipboard-manager|read_text");
    return t !== "jyroxa-e2e-sentinel" ? t : false;
  }, 5_000);
  if (text !== SMALL_URL) throw new Error(`clipboard holds ${text}`);
});

step("remove dialog Cancel keeps the item", async () => {
  await contextMenu(ids.small, "remove-list");
  await removeDialog("remove-cancel", "07-remove-dialog");
  await sleep(500);
  if (!(await item(ids.small))) throw new Error("item was removed after Cancel");
});

step("remove from list keeps the file", async () => {
  await contextMenu(ids.small, "remove-list");
  await removeDialog("remove-list-only");
  await waitFor("item gone", async () => !(await item(ids.small)), 5_000);
  if (!fs.existsSync(ids.smallPath)) throw new Error("file deleted although only list removal was chosen");
});

step("bulk remove and delete files (completed + failed)", async () => {
  const failedId = await addViaInvoke("https://proof.ovh.net/files/does-not-exist-jyroxa.dat", "missing.dat");
  await waitStatus(failedId, (s) => s === "failed", "failed", 60_000);
  const textPath = (await item(ids.text)).output;
  for (const id of [ids.text, failedId]) await cardAction(id, ".dl-select-checkbox");
  await waitFor("bulk bar", () => cdp.eval(`!document.getElementById("bulk-bar").classList.contains("hidden")`), 5_000);
  await cdp.shot("08-bulk-selected");
  await cdp.click("#bulk-remove");
  await removeDialog("remove-and-delete", "09-bulk-remove-dialog");
  await waitFor("both items gone", async () => {
    const l = await list();
    return !l.some((d) => d.id === ids.text || d.id === failedId);
  }, 10_000);
  if (fs.existsSync(textPath)) throw new Error(`file still on disk: ${textPath}`);
});

step("LinkGrabber splits a pasted URL list", async () => {
  await cdp.click("#btn-grabber");
  await sleep(400);
  await cdp.eval(`(() => { const el = document.getElementById("grab-input");
    el.value = ${JSON.stringify(`${SMALL_URL}\n${"https://proof.ovh.net/files/10Mb.dat"}`)};
    el.dispatchEvent(new Event("input", { bubbles: true })); return true; })()`);
  await cdp.click("#grab-run");
  await waitFor("2 grabbed links", () => cdp.eval(`document.querySelectorAll("#grab-packages-list .item-checkbox").length >= 2`), 30_000);
  await cdp.shot("10-linkgrabber");
});

step("LinkGrabber queues the selected links and they download", async () => {
  await cdp.eval(`(() => { const f = document.getElementById("grab-folder"); f.value = ${JSON.stringify(workDir)};
    f.dispatchEvent(new Event("input", { bubbles: true })); return true; })()`);
  const checked = await cdp.eval(`document.querySelectorAll("#grab-packages-list .item-checkbox:checked").length`);
  if (checked < 2) await cdp.click("#grab-sel-all");
  const before = new Set((await list()).map((d) => d.id));
  await cdp.click("#grab-queue-selected");
  const added = await waitFor("2 grabbed links in the queue", async () => {
    const fresh = (await list()).filter((d) => !before.has(d.id));
    return fresh.length >= 2 ? fresh : false;
  }, 15_000);
  const urls = added.flatMap((d) => d.urls);
  for (const u of [SMALL_URL, "https://proof.ovh.net/files/10Mb.dat"]) {
    if (!urls.includes(u)) throw new Error(`queue is missing ${u}: ${urls}`);
  }
  const outs = [];
  for (const d of added) {
    const fin = await waitStatus(d.id, done, "completed", 120_000);
    if (!fs.existsSync(fin.output) || fs.statSync(fin.output).size === 0) throw new Error(`no file for ${fin.output}`);
    outs.push(fin.output);
  }
  await cdp.shot("11-linkgrabber-queued");
  await cdp.invoke("remove_many", { ids: added.map((d) => d.id), deleteFile: true });
  const left = outs.filter((o) => fs.existsSync(o));
  if (left.length) throw new Error(`remove_many deleteFile=true left files: ${left}`);
});

step("settings saved from the Settings panel persist", async () => {
  const before = await cdp.invoke("get_settings");
  const want = before.max_concurrent === 4 ? 5 : 4;
  await cdp.click("#btn-settings");
  await sleep(400);
  await cdp.eval(`(() => { const el = document.getElementById("set-concurrent"); el.value = "${want}";
    el.dispatchEvent(new Event("input", { bubbles: true })); el.dispatchEvent(new Event("change", { bubbles: true })); return true; })()`);
  await cdp.click("#btn-save-settings");
  try {
    await waitFor("max_concurrent saved", async () => (await cdp.invoke("get_settings")).max_concurrent === want, 5_000);
    await cdp.shot("12-settings-saved");
  } finally {
    await cdp.invoke("set_settings", { settings: before });
  }
});

step("LinkGrabber page crawl lists files, not page assets", async () => {
  const pkgs = await cdp.invoke("grab_links", {
    input: "https://proof.ovh.net/files/",
    options: { deep: false, include_ext: null, min_size: null, max_size: null, host_filter: null, no_probe: true },
  });
  const urls = pkgs.flatMap((p) => (p.links || p.items || []).map((l) => l.url));
  if (!urls.some((u) => u.endsWith(".dat"))) throw new Error(`no .dat links: ${urls.slice(0, 5)}`);
  const junk = urls.filter((u) => /\.(css|js|woff2?|ico)(\?|$)/i.test(u));
  if (junk.length) throw new Error(`page assets listed: ${junk.slice(0, 5)}`);
});

async function main() {
  const failed = [];
  try {
    for (const s of steps) {
      process.stdout.write(`STEP ${s.name} ... `);
      try {
        await s.fn();
        console.log("PASS");
      } catch (e) {
        console.log(`FAIL\n    ${e.message}`);
        failed.push(s.name);
        try { await cdp?.shot(`fail-${steps.indexOf(s)}`); } catch {}
        if (!cdp) break;
      }
    }
    if (cdp?.errors.length) {
      console.log(`page errors:\n  ${cdp.errors.slice(0, 10).join("\n  ")}`);
      if (cdp.errors.some((e) => /invalid args|not allowed|command .* not found/i.test(e))) failed.push("page errors");
    }
  } finally {
    try {
      if (cdp && ids.big) await cdp.invoke("remove_download", { id: ids.big, deleteFile: true });
    } catch {}
    cdp?.ws?.close();
    if (child?.pid) {
      try { execFileSync("taskkill", ["/PID", String(child.pid), "/T", "/F"], { stdio: "ignore" }); } catch {}
    }
  }
  console.log(failed.length ? `\nE2E FAILED: ${failed.join("; ")}` : `\nE2E all ${steps.length} steps passed. Screenshots: ${shotsDir}`);
  process.exit(failed.length ? 1 : 0);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});