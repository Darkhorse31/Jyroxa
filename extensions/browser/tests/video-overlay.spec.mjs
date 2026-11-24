import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import {chromium} from "playwright";
import {repo, existingChromiumExecutable} from "./headless-engine.mjs";

const script=fs.readFileSync(path.join(repo,"extensions/browser/video-grab.js"),"utf8");
const css=fs.readFileSync(path.join(repo,"extensions/browser/video-grab.css"),"utf8");
const browser=await chromium.launch({executablePath:existingChromiumExecutable(),headless:true});
const page=await browser.newPage({viewport:{width:700,height:460}});
const errors=[];
page.on("pageerror",error=>errors.push(error.message));
const fixture=`<style>body{margin:0;min-height:1300px} #player{position:relative;transform:translateZ(0);z-index:0;overflow:hidden;width:620px;height:230px;margin-top:50px} video{width:620px;height:230px} #sidebar{position:fixed;left:560px;top:0;bottom:0;width:140px;background:#333;z-index:2147483647}</style><div id="player"><video></video></div><aside id="sidebar">Page sidebar</aside>`;
async function inject(frame, fallback=false) {
  await frame.evaluate(fallback=>{
    if(fallback) HTMLElement.prototype.showPopover=undefined;
    globalThis.handoffs=[];
    globalThis.chrome={runtime:{getURL:()=>"data:text/css,",sendMessage:async message=>{
      if(message.type==="video-grab-list")return {items:[1080,720,480,360,160].map(height=>({label:`${height}p MP4`,url:`https://example.org/${height}.mp4`,format_id:String(height)}))};
      if(message.type==="video-grab-handoff")globalThis.handoffs.push(message.payload);
      return {ok:true};
    },onMessage:{addListener(){}}},storage:{local:{get:async()=>({}),set:async()=>{}},onChanged:{addListener(listener){globalThis.changePrefs=listener;}}}};
  },fallback);
  await frame.addStyleTag({content:css});
  await frame.addScriptTag({content:script});
}
async function openAndCheck(frame) {
  await frame.locator(".jx-vgrab-btn").click();
  await frame.locator(".jx-vgrab-item").first().waitFor();
  const result=await frame.evaluate(()=>{
    const menu=document.querySelector(".jx-vgrab-menu");const r=menu.getBoundingClientRect();
    const row=menu.querySelector("button");const b=row.getBoundingClientRect();
    return {left:r.left,right:r.right,top:r.top,bottom:r.bottom,width:innerWidth,height:innerHeight,hit:document.elementFromPoint(b.left+b.width/2,b.top+b.height/2)===row};
  });
  assert.ok(result.left>=0&&result.right<=result.width&&result.top>=0&&result.bottom<=result.height,JSON.stringify(result));
  assert.equal(result.hit,true,"quality must receive clicks above the sidebar");
}
try {
  await page.setContent(fixture);await inject(page);await openAndCheck(page);
  assert.equal(await page.locator("#player").getAttribute("style"),null,"extension must not restyle the player");
  await page.locator(".jx-vgrab-item").first().click();
  assert.equal(await page.evaluate(()=>handoffs.at(-1).format_id),"1080");
  const button=page.locator(".jx-vgrab-btn");
  const anchor=await button.boundingBox();
  const observations=await page.evaluate(()=>{
    const button=document.querySelector(".jx-vgrab-btn");
    globalThis.hoverSamples=[];
    globalThis.hoverOriginal=button;
    globalThis.hoverTimer=setInterval(()=>{
      const box=button.getBoundingClientRect();
      hoverSamples.push(button.isConnected && box.width>0 && box.height>0
        && getComputedStyle(button).visibility!=="hidden" && document.querySelector(".jx-vgrab-btn")===button);
    },10);
    return document.querySelectorAll(".jx-vgrab-btn").length;
  });
  assert.equal(observations,1);
  for(let i=0;i<8;i++) {
    await page.mouse.move(anchor.x-35,anchor.y+40);
    await button.hover();
  }
  const samples=await page.evaluate(()=>{clearInterval(hoverTimer);return hoverSamples;});
  assert.ok(samples.length>0&&samples.every(Boolean),"hovering must not hide or replace the button");
  assert.equal(await page.locator(".jx-vgrab-menu").count(),0,"hover alone must not repeatedly open the menu");
  await page.evaluate(()=>changePrefs({jyroxaVideoGrab:{newValue:false}}));
  await page.locator("video").dispatchEvent("mouseenter");
  assert.equal(await button.count(),0,"disabled controls must not reappear on hover");
  await page.evaluate(()=>changePrefs({jyroxaVideoGrab:{newValue:true}}));
  assert.equal(await button.count(),1,"reenabling restores exactly one button");
  await page.evaluate(()=>document.querySelector("#player").style.transform="translateX(-40px)");
  await page.waitForFunction(()=>parseFloat(document.querySelector(".jx-vgrab-host").style.left)<540);
  await openAndCheck(page);await page.keyboard.press("ArrowDown");
  assert.equal(await page.locator(".jx-vgrab-item").first().evaluate(el=>el===document.activeElement),true);
  await page.keyboard.press("Escape");assert.equal(await page.locator(".jx-vgrab-menu").count(),0);
  assert.equal(await page.locator(".jx-vgrab-btn").evaluate(el=>el===document.activeElement),true);
  await openAndCheck(page);await page.mouse.click(15,400);assert.equal(await page.locator(".jx-vgrab-menu").count(),0);
  await page.setViewportSize({width:330,height:260});await openAndCheck(page);
  await page.evaluate(()=>scrollTo(0,80));
  await page.waitForFunction(()=>document.querySelector(".jx-vgrab-host").getBoundingClientRect().top<=7);
  const scrolled=await page.locator(".jx-vgrab-menu").boundingBox();assert.ok(scrolled.x>=0&&scrolled.x+scrolled.width<=330);
  await page.keyboard.press("Escape");
  await page.evaluate(()=>document.querySelector("#player").requestFullscreen());
  await page.waitForFunction(()=>document.fullscreenElement?.contains(document.querySelector(".jx-vgrab-host")));
  await openAndCheck(page);await page.keyboard.press("Escape");await page.evaluate(()=>document.exitFullscreen());
  await page.setViewportSize({width:700,height:460});await page.setContent(`<iframe style="width:260px;height:240px" srcdoc='${fixture.replaceAll("'","&#39;")}'></iframe>`);
  const frame=page.frames().find(frame=>frame!==page.mainFrame());await frame.waitForSelector("video");await inject(frame);await openAndCheck(frame);
  await frame.locator(".jx-vgrab-item").first().click();assert.equal(await frame.evaluate(()=>handoffs.length),1);
  await page.setContent(fixture);await inject(page,true);await openAndCheck(page);
  assert.deepEqual(errors,[]);
  console.log("video-overlay.spec.mjs OK: stable hover, stacking, clipping, edge, resize, scroll, fullscreen, iframe, keyboard, handoff and fixed fallback");
} finally {await browser.close();}
