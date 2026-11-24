/** Live YouTube and Dailymotion extraction through the actual headless engine. */
import assert from "node:assert/strict";
import { startEngine } from "./headless-engine.mjs";

const engine=await startEngine();
const url="https://www.youtube.com/watch?v=jNQXAC9IVRw";
try {
  assert.deepEqual(engine.native({type:"ping"}),{ok:true,connected:true});
  const probe=await engine.request({command:"probe",url});
  assert.equal(probe.ok,true,JSON.stringify(probe));
  assert.equal(probe.result.server,"yt-dlp");
  assert.ok(probe.result.suggested_name.includes("Me at the zoo"));
  assert.ok(probe.result.video_formats.length>=2);
  console.log("desktop Add Download metadata",JSON.stringify(probe));
  for(const page of ["https://youtu.be/jNQXAC9IVRw","https://www.youtube.com/shorts/jNQXAC9IVRw"]) {
    const info=await engine.request({command:"probe",url:page});
    assert.equal(info.ok,true,JSON.stringify(info));assert.ok(info.result.video_formats.length>=2);
  }
  const grab=await engine.request({command:"grab",url});assert.equal(grab.ok,true,JSON.stringify(grab));
  const link=grab.result.flatMap(pkg=>pkg.links).find(link=>link.url===url);
  assert.ok(link?.filename.includes("Me at the zoo"));assert.equal(link.mime,"video/mp4");
  const yt=engine.native({type:"probe_video_formats",page_url:url});
  assert.equal(yt.ok,true,JSON.stringify(yt));assert.ok(yt.formats.length>=2);
  console.log("extension metadata",JSON.stringify(yt));
  let dm=engine.native({type:"probe_video_formats",page_url:"https://www.dailymotion.com/video/x89eyek"});
  if(!dm.ok){await new Promise(resolve=>setTimeout(resolve,2000));dm=engine.native({type:"probe_video_formats",page_url:"https://www.dailymotion.com/video/x89eyek"});}
  assert.equal(dm.ok,true,JSON.stringify(dm));assert.ok(dm.formats.length>=1);
  console.log("Dailymotion metadata",JSON.stringify(dm));
  const added=await engine.request({command:"add",args:{urls:[url],output_dir:engine.downloads,filename:"desktop-video.mp4",connections:1,connections_auto:true}});
  assert.equal(added.ok,true,JSON.stringify(added));engine.decode(await engine.waitFor(added.result));
  const browserSettings = engine.native({type:"get_settings"});
  assert.equal(browserSettings.ok,true,JSON.stringify(browserSettings));
  // Configure the isolated engine directly so setup cannot touch Windows startup settings.
  const configured = await engine.request({command:"settings",settings:{browser_integration:{...browserSettings.settings,silent_add:true}}});
  assert.equal(configured.ok,true,JSON.stringify(configured));
  // Distinct input aliases avoid the app's intentional same-URL deduplication.
  const handoff=engine.native({type:"handoff",url:"https://youtu.be/jNQXAC9IVRw",page_url:"https://example.org/embedding-page",filename:"browser-video.mp4",video_page:true,format_id:"bv*+ba/b"});
  assert.equal(handoff.ok,true,JSON.stringify(handoff));assert.ok(handoff.download_id);
  assert.notEqual(handoff.download_id, added.result, "browser handoff must create and finish its own transfer");
  engine.decode(await engine.waitFor(handoff.download_id));
  const audio = probe.result.video_formats.find(format => /audio only/i.test(format.label));
  assert.ok(audio, "audio quality must be offered");
  const audioAdded = await engine.request({command:"add",args:{urls:["https://www.youtube.com/shorts/jNQXAC9IVRw"],output_dir:engine.downloads,filename:"audio-100%.mp4",connections:1,connections_auto:true,http_headers:{"X-Jyroxa-Ytdlp-Format":audio.format_id}}});
  assert.equal(audioAdded.ok,true,JSON.stringify(audioAdded));
  engine.decode(await engine.waitFor(audioAdded.result),true);
  const journal = await engine.request({command:"progress_events"});
  assert.equal(journal.ok,true,JSON.stringify(journal));
  for (const id of [added.result, handoff.download_id, audioAdded.result]) {
    const updates = journal.result.filter(entry=>entry.id===id && entry.event.type==="video_progress").map(entry=>entry.event.progress);
    assert.ok(updates.some(p=>p.phase==="fetching"), `fetching status for ${id}`);
    assert.ok(updates.some(p=>p.phase==="downloading" && p.downloaded>0 && p.total>p.downloaded), `intermediate progress for ${id}`);
    assert.ok(updates.some(p=>p.phase==="processing" || p.phase==="merging"), `processing status for ${id}`);
    console.log("live video progress",JSON.stringify({id,updates}));
  }
  console.log("video-menu-probe.spec.mjs OK: desktop, short URL, shorts, Link Grabber, native probe and browser handoff decoded");
} finally { await engine.close(); }
