import assert from "node:assert/strict";
import path from "node:path";
import {fileURLToPath} from "node:url";
import {createServer} from "vite";
import {chromium} from "playwright";
import {existingChromiumExecutable} from "../../../extensions/browser/tests/headless-engine.mjs";

const desktop=path.resolve(path.dirname(fileURLToPath(import.meta.url)),"..");
const server=await createServer({root:desktop,server:{host:"127.0.0.1",port:4186,strictPort:true},clearScreen:false});
let browser;
try {
  await server.listen();
  browser=await chromium.launch({executablePath:existingChromiumExecutable(),headless:true});
  const page=await browser.newPage();
  // Serve an empty document; load only the actual task-card module, not app startup.
  await page.route("http://127.0.0.1:4186/card-test",route=>route.fulfill({contentType:"text/html",body:"<!doctype html><body></body>"}));
  await page.goto("http://127.0.0.1:4186/card-test");
  const results=await page.evaluate(async()=>{
    const {DownloadCard}=await import("/src/download-card.ts");
    const view={id:1,urls:["https://youtu.be/example"],filename:"video.mp4",output:"C:/test/video.mp4",connections:8,connections_auto:true,status:"running",downloaded:0,total:null,speed_bps:0,segments:[]};
    const callbacks={onSelect(){},onChanged(){},onContextMenu(){}};
    const card=new DownloadCard(view,callbacks);document.body.append(card.root);
    const progress={phase:"downloading",downloaded:512,total:1024,total_is_estimate:false,speed_bps:256,eta_secs:2};
    card.applyProgressEvent({type:"video_progress",progress});
    const transfer={summary:card.root.querySelector(".dl-summary").textContent,percent:card.root.querySelector('[data-slot="pct"]').textContent,speed:card.root.querySelector('[data-slot="speed"]').textContent};
    const snapshot=new DownloadCard({...view,downloaded:512,total:1024,speed_bps:256,video_progress:progress},callbacks);
    const restored=snapshot.root.querySelector(".dl-summary").textContent;
    const restoredEta=snapshot.root.querySelector('[data-slot="eta"]').textContent;
    card.applyProgressEvent({type:"video_progress",progress:{...progress,phase:"merging",downloaded:1024,speed_bps:0,eta_secs:null}});
    const merging={summary:card.root.querySelector(".dl-summary").textContent,status:card.root.dataset.status};
    card.applyProgressEvent({type:"completed",total_size:1010});
    const completed={summary:card.root.querySelector(".dl-summary").textContent,busy:card.root.querySelector(".progress-track").hasAttribute("aria-busy")};
    return {transfer,restored,restoredEta,merging,completed};
  });
  assert.equal(results.transfer.percent,"50.0%");
  assert.match(results.transfer.summary,/remaining, 2 seconds left/);
  assert.notEqual(results.transfer.speed,"0 B/s");
  assert.equal(results.restored,results.transfer.summary);
  assert.notEqual(results.restoredEta,"—");
  assert.match(results.merging.summary,/Merging audio and video/);
  assert.equal(results.merging.status,"downloading");
  assert.deepEqual(results.completed,{summary:"Completed",busy:false});
  console.log("video-progress-ui.mjs OK",JSON.stringify(results));
} finally {if(browser)await browser.close();await server.close();}
