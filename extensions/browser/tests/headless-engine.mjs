import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

export const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
export function existingChromiumExecutable() {
  const cache = path.join(process.env.LOCALAPPDATA, "ms-playwright");
  const candidates = fs.readdirSync(cache).filter(name => /^chromium-\d+$/.test(name)).sort((a,b) => Number(b.split("-")[1])-Number(a.split("-")[1]));
  const executable = candidates.flatMap(name => ["chrome-win64/chrome.exe","chrome-win/chrome.exe"].map(file => path.join(cache,name,file))).find(file => fs.existsSync(file));
  assert.ok(executable, "An existing headless Chromium is required; this test never installs a browser");
  return executable;
}
export async function startEngine() {
  const target = process.env.CARGO_TARGET_DIR || path.join(repo, "target");
  const stage = fs.mkdtempSync(path.join(os.tmpdir(), "jyroxa-video-test-"));
  for (const name of ["jyroxa-release-gate.exe", "jyroxa-native-host.exe"]) {
    const source = path.join(target, "release", name);
    assert.ok(fs.existsSync(source), `Build release-gate and native host: ${source}`);
    fs.copyFileSync(source, path.join(stage, name));
  }
  for (const name of ["yt-dlp.exe", "ffmpeg.exe", "node.exe", "WebView2Loader.dll"]) {
    const source = path.join(repo, "apps/desktop/src-tauri", name);
    assert.ok(fs.existsSync(source), `Missing sidecar: ${source}`);
    fs.copyFileSync(source, path.join(stage, name));
  }
  const data = path.join(stage, "data");
  const downloads = path.join(stage, "downloads"); fs.mkdirSync(downloads);
  const env = { ...process.env, JYROXA_DATA_DIR:data, JYROXA_NATIVE_HOST_NO_LAUNCH:"1",
    JYROXA_HANDOFF_PIPE: String.raw`\\.\pipe\jyroxa-test-video-${process.pid}` };
  const child = spawn(path.join(stage,"jyroxa-release-gate.exe"), [], {env,windowsHide:true,stdio:["pipe","pipe","pipe"]});
  let stderr=""; child.stderr.on("data",chunk=>stderr+=chunk);
  const lines=createInterface({input:child.stdout});
  const pending=[];
  lines.on("line",line=>{ const next=pending.shift(); if(next) {clearTimeout(next.timer); next.resolve(JSON.parse(line));} });
  child.on("exit",code=>{for(const next of pending.splice(0)){clearTimeout(next.timer);next.reject(new Error(`engine exited ${code}: ${stderr}`));}});
  async function request(input) {
    return new Promise((resolve,reject)=>{
      const timer=setTimeout(()=>{child.kill();reject(new Error(`engine timeout: ${JSON.stringify(input)} ${stderr}`));},180000);
      pending.push({resolve,reject,timer});child.stdin.write(JSON.stringify(input)+"\n");
    });
  }
  const ready=await request({command:"list"});assert.equal(ready.ok,true,JSON.stringify(ready));
  const settings=await request({command:"settings",settings:{download_dir:downloads,notifications:false}});assert.equal(settings.ok,true,JSON.stringify(settings));
  const native = payload => {
    const body=Buffer.from(JSON.stringify(payload));const size=Buffer.alloc(4);size.writeUInt32LE(body.length);
    const result=spawnSync(path.join(stage,"jyroxa-native-host.exe"),[],{env,windowsHide:true,input:Buffer.concat([size,body]),timeout:180000,maxBuffer:4*1024*1024});
    assert.equal(result.status,0,result.stderr?.toString()); assert.ok(result.stdout.length>=4,"native host must answer");
    assert.equal(result.stdout.readUInt32LE(0),result.stdout.length-4,"exactly one native frame");
    return JSON.parse(result.stdout.subarray(4).toString());
  };
  const waitFor = async id => {
    for(let i=0;i<180;i++) {
      const reply=await request({command:"list"});assert.equal(reply.ok,true,JSON.stringify(reply));
      const row=reply.result.find(row=>row.id===id);
      assert.ok(row,`download ${id} must exist`);
      assert.notEqual(row.status,"failed",JSON.stringify(row));
      if(row.status==="completed")return row;
      await new Promise(resolve=>setTimeout(resolve,500));
    }
    assert.fail(`download ${id} did not complete`);
  };
  const decode = (row, audioOnly = false) => {
    assert.ok(path.resolve(row.output).startsWith(path.resolve(downloads)+path.sep));
    const bytes=fs.statSync(row.output).size;assert.ok(bytes>10000);assert.equal(row.downloaded,bytes);
    const result=spawnSync(path.join(stage,"ffmpeg.exe"),["-v","error","-i",row.output,...(audioOnly ? [] : ["-map","0:v:0"]),"-map","0:a:0","-f","null","-"],{windowsHide:true,timeout:90000});
    assert.equal(result.status,0,result.stderr?.toString());assert.equal(result.stderr.length,0);
    console.log(JSON.stringify({id:row.id,bytes,audio_only:audioOnly,decode_exit:result.status,decode_stderr:result.stderr.toString()}));
  };
  return {request,native,waitFor,decode,stage,downloads,child,async close(){child.kill();await new Promise(resolve=>child.exitCode!==null?resolve():child.once("exit",resolve));lines.close();assert.equal(path.dirname(path.resolve(stage)),path.resolve(os.tmpdir()));fs.rmSync(stage,{recursive:true,force:true,maxRetries:5,retryDelay:200});}};
}
