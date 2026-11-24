/** Chromium parser and transport regression, using a unique temporary host name. */
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import net from "node:net";
import { spawnSync } from "node:child_process";
import { chromium } from "playwright";
import { repo, existingChromiumExecutable } from "./headless-engine.mjs";
import { connectionFailureReason } from "../native.js";

const stage = fs.mkdtempSync(path.join(os.tmpdir(), "jyroxa-browser-test-"));
const hostName = `com.jyroxa.test.p${process.pid}`;
const keys = ["Google\\Chrome", "Chromium"].map(vendor => `HKCU\\Software\\${vendor}\\NativeMessagingHosts\\${hostName}`);
const manifestPath = path.join(stage, "host.json");
const data = path.join(stage, "data"); fs.mkdirSync(data);
fs.writeFileSync(path.join(data, "browser-integration.token"), "isolated-browser-test");
const pipe = String.raw`\\.\pipe\jyroxa-test-browser-${process.pid}`;
const env = { ...process.env, JYROXA_DATA_DIR: data, JYROXA_HANDOFF_PIPE: pipe, JYROXA_NATIVE_HOST_NO_LAUNCH: "1" };
const extension = path.join(stage, "extension"); fs.mkdirSync(extension);
const shipped = JSON.parse(fs.readFileSync(path.join(repo, "extensions/browser/manifest.json"), "utf8"));
fs.writeFileSync(path.join(extension, "manifest.json"), JSON.stringify({manifest_version:3,name:"Jyroxa native transport test",version:"1.0",key:shipped.key,permissions:["nativeMessaging"],background:{service_worker:"probe.js"}}));
fs.writeFileSync(path.join(extension, "probe.js"), "chrome.runtime.onInstalled.addListener(()=>{});");
const target = process.env.CARGO_TARGET_DIR || path.join(repo, "target");
const host = path.join(target, "release/jyroxa-native-host.exe"); assert.ok(fs.existsSync(host));
const baseManifest = {name:hostName,description:"Isolated native transport test",type:"stdio",path:host,allowed_origins:["chrome-extension://njgbdpacpmohafoefmcmnmhdjpjmjipe/"]};
const reg = args => spawnSync("reg.exe", args, {windowsHide:true,encoding:"utf8",timeout:10000});
const server = net.createServer(socket => {
  let buffer = "";
  socket.on("data", chunk => {
    buffer += chunk;
    if (!buffer.includes("\n")) return;
    const message = JSON.parse(buffer.trim());
    assert.equal(message.token, "isolated-browser-test"); assert.equal(message.type, "ping");
    socket.end(JSON.stringify({ok:true,connected:true}) + "\n");
  });
});
let browser;
const created = [];
try {
  for (const key of keys) {
    assert.notEqual(reg(["query", key]).status, 0, "test host name must be unused");
    for (const view of [32,64]) {
      const result = reg(["add", key, "/ve", "/t", "REG_SZ", "/d", manifestPath, "/f", `/reg:${view}`]);
      assert.equal(result.status, 0, result.stderr); created.push([key,view]);
    }
  }
  await new Promise((resolve,reject) => {server.once("error",reject);server.listen(pipe,resolve);});
  const executablePath = existingChromiumExecutable();
  browser = await chromium.launchPersistentContext(path.join(stage,"profile"), {executablePath,headless:true,env,args:["--headless=new",`--disable-extensions-except=${extension}`,`--load-extension=${extension}`,"--no-sandbox"]});
  const worker = browser.serviceWorkers()[0] || await browser.waitForEvent("serviceworker",{timeout:20000});
  const ping = () => worker.evaluate(name => new Promise(resolve => chrome.runtime.sendNativeMessage(name,{type:"ping"},reply => resolve({id:chrome.runtime.id,reply,error:chrome.runtime.lastError?.message}))),hostName);
  for (const [label, manifest, expected] of [
    ["mixed Firefox scheme", {...baseManifest,allowed_origins:[...baseManifest.allowed_origins,"moz-extension://jyroxa-browser@jyroxa/"]}, "Host unavailable"],
    ["forbidden extension ID", {...baseManifest,allowed_origins:[`chrome-extension://${"a".repeat(32)}/`]}, "Extension not allowed"],
    ["missing host file", {...baseManifest,path:path.join(stage,"missing.exe")}, "Host unavailable"],
  ]) {
    fs.writeFileSync(manifestPath,JSON.stringify(manifest));
    const result = await ping();
    assert.equal(result.id,"njgbdpacpmohafoefmcmnmhdjpjmjipe");
    assert.equal(connectionFailureReason(result.error).short,expected,JSON.stringify(result));
    console.log(label,JSON.stringify(result));
  }
  fs.writeFileSync(manifestPath,JSON.stringify(baseManifest));
  const result = await ping(); assert.deepEqual(result.reply,{ok:true,connected:true});assert.equal(result.error,undefined);
  console.log("valid Chromium manifest",JSON.stringify(result));
  console.log("native-browser.spec.mjs OK");
} finally {
  if (browser) await browser.close();
  server.close();
  for (const [key,view] of created) reg(["delete",key,"/f",`/reg:${view}`]);
  assert.equal(path.dirname(path.resolve(stage)),path.resolve(os.tmpdir()));
  fs.rmSync(stage,{recursive:true,force:true,maxRetries:5,retryDelay:200});
}
