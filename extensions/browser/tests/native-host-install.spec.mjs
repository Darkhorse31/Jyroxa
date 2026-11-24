/** Isolated installer registration, browser discovery and native framing. */
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import net from "node:net";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const target = process.env.CARGO_TARGET_DIR || path.join(repoRoot, "target");
const hostExe = path.join(target, "release/jyroxa-native-host.exe");
const registerExe = path.join(target, "release/jyroxa-browser-register.exe");
for (const exe of [hostExe, registerExe]) assert.ok(fs.existsSync(exe), `Build release binaries first: ${exe}`);
const stage = fs.mkdtempSync(path.join(os.tmpdir(), "jyroxa-native-test-"));
const data = path.join(stage, "data");
fs.mkdirSync(data);
const desktop = path.join(stage, "jyroxa-desktop.exe");
fs.writeFileSync(desktop, "stub");
fs.copyFileSync(hostExe, path.join(stage, "jyroxa-native-host.exe"));
const prefix = `JyroxaHostTest${process.pid}`;
const pipe = `\\\\.\\pipe\\jyroxa-test-install-${process.pid}`;
const env = { ...process.env, LOCALAPPDATA: path.join(stage, "profiles"), JYROXA_DATA_DIR: data,
  JYROXA_REGISTRY_PREFIX: prefix, JYROXA_NATIVE_HOST_NO_LAUNCH: "1", JYROXA_HANDOFF_PIPE: pipe };
const browsers = ["Google/Chrome", "Microsoft/Edge", "BraveSoftware/Brave-Browser", "Chromium"];
const ids = ["a", "b", "c", "d"].map(letter => letter.repeat(32));
for (const [index, browser] of browsers.entries()) {
  const profile = path.join(env.LOCALAPPDATA, browser, "User Data/Profile 2");
  const unpacked = path.join(stage, `extension-${index}`);
  fs.mkdirSync(profile, {recursive:true}); fs.mkdirSync(unpacked);
  fs.writeFileSync(path.join(unpacked, "manifest.json"), JSON.stringify({name:"Jyroxa Download Integration"}));
  fs.writeFileSync(path.join(profile, index % 2 ? "Preferences" : "Secure Preferences"), JSON.stringify({extensions:{settings:{[ids[index]]:{path:unpacked}}}}));
}
const run = args => spawnSync(registerExe, args, {env, windowsHide:true, encoding:"utf8", timeout:30000});
const frame = value => { const body=Buffer.from(JSON.stringify(value)); const size=Buffer.alloc(4); size.writeUInt32LE(body.length); return Buffer.concat([size,body]); };
const server = net.createServer(socket => {
  let buffer="";
  socket.on("data", chunk => {
    buffer+=chunk;
    if (!buffer.includes("\n")) return;
    const envelope=JSON.parse(buffer.trim());
    assert.equal(envelope.token, fs.readFileSync(path.join(data,"browser-integration.token"),"utf8"));
    assert.equal(envelope.type, "ping");
    socket.end(JSON.stringify({ok:true,connected:true})+"\n");
  });
});
try {
  const install=run(["install",desktop]); assert.equal(install.status,0,install.stderr);
  const manifestPath=path.join(data,"native-host/com.jyroxa.browser.json");
  const manifest=JSON.parse(fs.readFileSync(manifestPath,"utf8"));
  assert.equal(manifest.name,"com.jyroxa.browser"); assert.equal(manifest.type,"stdio");
  assert.ok(path.isAbsolute(manifest.path)); assert.ok(fs.existsSync(manifest.path));
  assert.equal(manifest.path, path.join(data, "jyroxa-native-host.exe"), "manifest must use the stable copied host");
  assert.equal(fs.readFileSync(path.join(data,"desktop-exe.txt"),"utf8"),desktop);
  for(const id of ["njgbdpacpmohafoefmcmnmhdjpjmjipe","hehgieooaeknhlodfpkommnhinaahkjf","lkcpmllaclbmggbphpaojdklcjgmjaad",...ids]) assert.ok(manifest.allowed_origins.includes(`chrome-extension://${id}/`),id);
  assert.equal(new Set(manifest.allowed_origins).size, manifest.allowed_origins.length);
  for (const origin of manifest.allowed_origins) assert.match(origin, /^chrome-extension:\/\/[a-p]{32}\/$/, "Chromium rejects non-extension schemes even if the caller ID is also present");
  const firefoxPath=path.join(data,"native-host/com.jyroxa.browser.firefox.json");
  const firefox=JSON.parse(fs.readFileSync(firefoxPath,"utf8"));
  assert.deepEqual(firefox.allowed_extensions,["jyroxa-browser@jyroxa"]);
  assert.equal(firefox.allowed_origins,undefined);
  for(const browser of [...browsers,"Mozilla"]) for(const view of [32,64]) {
    const key=`HKCU\\Software\\${prefix}\\${browser.replaceAll("/","\\")}\\NativeMessagingHosts\\com.jyroxa.browser`;
    const query=spawnSync("reg.exe",["query",key,"/ve",`/reg:${view}`],{windowsHide:true,encoding:"utf8"});
    assert.equal(query.status,0,query.stderr); assert.ok(query.stdout.includes(browser === "Mozilla" ? firefoxPath : manifestPath));
  }
  const missing=run(["install",path.join(stage,"missing.exe")]); assert.notEqual(missing.status,0);
  assert.deepEqual(JSON.parse(fs.readFileSync(manifestPath,"utf8")),manifest);
  await new Promise((resolve,reject)=>{ server.once("error",reject);server.listen(pipe,resolve); });
  const reply=await new Promise((resolve,reject)=>{
    const child=spawn(manifest.path,[],{env,windowsHide:true,stdio:["pipe","pipe","pipe"]});
    const chunks=[];let stderr="";
    const timeout=setTimeout(()=>{child.kill();reject(new Error("native ping timed out"));},10000);
    child.stdout.on("data",chunk=>chunks.push(chunk));child.stderr.on("data",chunk=>stderr+=chunk);
    child.once("error",reject);child.once("exit",code=>{clearTimeout(timeout);try {
      assert.equal(code,0,stderr); const output=Buffer.concat(chunks);assert.equal(output.readUInt32LE(0),output.length-4);
      assert.equal(stderr,"");resolve(JSON.parse(output.subarray(4).toString()));
    } catch(error){reject(error);} });child.stdin.end(frame({type:"ping"}));
  });
  assert.deepEqual(reply,{ok:true,connected:true});
  console.log("native-host-install.spec.mjs OK: 5 browser keys x 2 registry views, 4 discovered IDs, exact framed ping", JSON.stringify(reply));
} finally {
  server.close(); const cleanup=run(["uninstall"]);assert.equal(cleanup.status,0,cleanup.stderr);
  for(const browser of [...browsers,"Mozilla"]) {
    const key=`HKCU\\Software\\${prefix}\\${browser.replaceAll("/","\\")}\\NativeMessagingHosts\\com.jyroxa.browser`;
    const query=spawnSync("reg.exe",["query",key],{windowsHide:true});assert.notEqual(query.status,0,"test registration must be removed");
  }
  assert.equal(path.dirname(path.resolve(stage)),path.resolve(os.tmpdir()));
  fs.rmSync(stage,{recursive:true,force:true,maxRetries:5,retryDelay:200});
}
