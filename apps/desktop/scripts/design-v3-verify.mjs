import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";
import { chromium } from "playwright";

const desktop = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const reportDir = process.env.DESIGN_REPORT_DIR || "C:/Users/lenovo/.jyroxa-tools/reports/design-v3";
fs.mkdirSync(reportDir, { recursive: true });
const server = spawn(process.execPath, [path.join(desktop, "node_modules/vite/bin/vite.js"), "preview", "--host", "127.0.0.1", "--port", "4178", "--strictPort"], { cwd: desktop, windowsHide: true, stdio: "pipe" });
let serverOutput = "";
server.stdout.on("data", data => { serverOutput += data; });
server.stderr.on("data", data => { serverOutput += data; });
let browser;
let browserServer;
const shots = [];
const errors = [];
let assertions = 0;
const check = (actual, expected, label) => { assert.deepEqual(actual, expected, label); assertions++; };
try {
  const address = "http://127.0.0.1:4178";
  for (let attempt = 0; ; attempt++) {
    try { if ((await fetch(address)).ok) break; } catch { /* Server starting. */ }
    if (attempt > 300 || server.exitCode !== null) throw new Error(`Preview unavailable: ${serverOutput}`);
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  browserServer = await chromium.launchServer({ channel: "msedge", headless: true });
  browser = await chromium.connect(browserServer.wsEndpoint());
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1, reducedMotion: "reduce" });
  await context.addInitScript(() => {
    const MiB = 1024 ** 2, GiB = 1024 ** 3;
    const create = (id, filename, status, downloaded, total, speed_bps, extra = {}) => ({ id, filename, status, downloaded, total, speed_bps, urls: [`https://downloads.example.org/${filename}`], output: `C:\\Downloads\\${filename}`, connections: 8, connections_auto: true, segments: [], ...extra });
    const initial = [
      create(6, "ubuntu-24.04-desktop-amd64.iso", "downloading", 3.7*GiB,5.8*GiB,92.4*MiB, { segments: Array.from({length:8},(_,i)=>({index:i,start:i*778462822,end:(i+1)*778462822,downloaded:i<5?778462822:0})) }),
      create(5, "Blender Open Movie 4K.mp4", "downloading",1.2*GiB,2.6*GiB,65.1*MiB,{kind:"video"}),
      create(4, "debian-13-netinst.iso", "downloading",512*MiB,1.1*GiB,24.7*MiB,{kind:"torrent",urls:["magnet:?xt=urn:btih:0123456789012345678901234567890123456789"],torrent_peers:12,torrent_seeds:4,torrent_ratio:0.12}),
      create(3, "project-assets.zip", "completed",126*MiB,126*MiB,0),
      create(2, "installer.msi", "paused",12*MiB,48*MiB,0),
      create(1, "reference-photos.zip", "failed",4*MiB,50*MiB,0,{error:"HTTP 502: the server is temporarily unavailable"}),
      create(0, "monthly-backup.zip", "queued",0,145*MiB,0),
    ];
    let downloads = structuredClone(initial);
    const settings = { max_concurrent:4,max_connections_per_download:8,per_host_cap:8,global_speed_limit:null,download_dir:"C:\\Downloads",default_folder:"C:\\Downloads",category_dirs:{Video:"C:\\Downloads\\Videos",Music:"C:\\Downloads\\Music",Documents:"C:\\Downloads\\Documents",Compressed:"C:\\Downloads\\Archives",Programs:"C:\\Downloads\\Programs",Other:"C:\\Downloads"},clipboard_monitor:true,autotune:true,start_minimized:false,notifications:false,close_to_tray:true,start_with_windows:false,prevent_sleep:true,auto_resume:true,theme:"light",scheduler_start:"23:00",scheduler_stop:"07:00",scheduler_days:[1,2,3,4,5],browser_integration:{enabled:true,min_size_bytes:0,file_types:{archives:true,programs:true,video:true,audio:true,documents:true,iso:true},silent_add:false,disabled_hosts:[],chrome_extension_id:"",edge_extension_id:"",video_grab_enabled:true} };
    const callbacks = new Map(), listeners = new Map();
    let callbackId = 0;
    const emit = (name, payload) => { for (const id of listeners.get(name) || []) callbacks.get(id)?.({event:name,id:1,payload}); };
    window.__design = { calls:[],clipboard:"",failProbe:false,failAdd:false,emit,reset:()=>{downloads=structuredClone(initial);emit("downloads-changed");},empty:()=>{downloads=[];emit("downloads-changed");} };
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    window.__TAURI_INTERNALS__ = {
      transformCallback(callback) { const id = ++callbackId; callbacks.set(id, callback); return id; },
      unregisterCallback(id) { callbacks.delete(id); },
      async invoke(command, args = {}) {
        window.__design.calls.push({command,args});
        if (command === "plugin:app|version") return "0.1.11";
        if (command === "plugin:event|listen") { listeners.set(args.event,[...(listeners.get(args.event)||[]),args.handler]); return 1; }
        if (command === "plugin:event|unlisten") return;
        if (command === "plugin:clipboard-manager|read_text") return window.__design.clipboard;
        if (command === "plugin:clipboard-manager|write_text") {window.__design.clipboard=args.text;return;}
        if (command === "plugin:dialog|open") return "C:\\Downloads\\release.torrent";
        if (command === "get_settings") return structuredClone(settings);
        if (command === "set_settings") { Object.assign(settings,args.settings);return; }
        if (command === "list_downloads") return structuredClone(downloads);
        if (command === "default_category_dirs") return settings.category_dirs;
        if (command === "suggest_save_path") return `${args.dir}\\${args.name}`;
        if (["probe_url","probe_url_with_headers"].includes(command)) {
          if (args.url.includes("slow.zip")) await new Promise(resolve => { window.__design.releaseProbe=resolve; });
          if (window.__design.failProbe) throw new Error("The server could not be reached. Check your connection and try again.");
          return {final_url:args.url,suggested_name:args.url.split("/").pop()||"download",size:5.8*GiB,mime:args.url.endsWith("mp4")?"video/mp4":"application/octet-stream",resumable:true,category:args.url.endsWith("mp4")?"Video":"Other",server:"downloads.example.org",protocol:"HTTPS"};
        }
        if (command === "probe_torrent") return {name:"Debian release",total_bytes:1.1*GiB,files:[{index:0,path:"debian-13-netinst.iso",length:GiB},{index:1,path:"README.txt",length:2048},{index:2,path:"SHA256SUMS",length:256}]};
        if (command === "add_download") {
          if (window.__design.failAdd) throw new Error(window.__design.addError || "Could not save the download. Check the folder permissions.");
          downloads.push(create(20,args.args.filename,"queued",0,0,0,{urls:args.args.urls})); return 20;
        }
        if (["pause_many","resume_many","pause_download","resume_download","retry_download","cancel_download","cancel_many"].includes(command)) {
          const ids=args.ids||[args.id];
          for (const item of downloads) if(ids.includes(item.id)){item.status=command.startsWith("pause")?"paused":command.startsWith("cancel")?"cancelled":"downloading";item.speed_bps=0;}
          emit("downloads-changed");return ids.length;
        }
        if (command === "remove_many" || command === "remove_download") { const ids=args.ids||[args.id];downloads=downloads.filter(item=>!ids.includes(item.id));return ids.length; }
        if (command === "open_folder_cmd" || command === "open_file") return;
        if (command === "get_browser_integration_status") return {token_present:true,host_manifest_present:true,chrome_registry:true,edge_registry:true,firefox_registry:false,native_host_installed:true,packaged_extension_id:""};
        if (command === "telegram_status") return {authorized:false,has_api_credentials:false,phone:null};
        if (command === "probe_links") return args.urls.map(url=>({url,filename:url.split("/").pop(),size:104857600,mime:"application/zip",resumable:true}));
        if (command === "grab_links") return [{name:"Files on this page",links:[{url:"https://example.org/movie.mp4",filename:"movie.mp4",size:104857600,resumable:true}]}];
        throw new Error(`Unhandled preview command: ${command}`);
      },
    };
  });
  const page = await context.newPage();
  page.on("pageerror", error => errors.push(error.message));
  await page.goto(address, {waitUntil:"networkidle"});
  await page.locator('.dl-card[data-id="6"]').waitFor();
  const shot = async (theme,name) => {
    await page.mouse.move(10,10);
    await page.screenshot({ path:path.join(reportDir,`${theme}-${name}.png`), animations:"disabled" });
    shots.push(`${theme}-${name}.png`);
    console.log(`Captured ${theme}-${name}`);
  };
  const theme = async value => {
    await page.evaluate(t=>{ document.documentElement.dataset.theme=t;document.body.dataset.theme=t; },value);
  };
  const close = async () => {
    await page.evaluate(()=>{document.querySelectorAll("dialog[open]").forEach(dialog=>dialog.close());document.getElementById("sidebar-close").click();document.querySelectorAll(".toast").forEach(toast=>toast.remove());document.querySelector(".speed-details").open=false;document.activeElement?.blur();});
  };
  check(await page.locator(".dl-card").count(),7,"all fixture downloads rendered");
  check(await page.locator("#badge-status-downloading").textContent(),"3","active count");
  check(await page.locator("#badge-status-torrents").textContent(),"1","torrent count");
  check(await page.locator('.dl-card[data-id="6"] .dl-summary').textContent(),"3.7 GB of 5.8 GB, 24 seconds left","readable progress");
  for (const value of ["light","dark"]) {
    await close();await theme(value);await shot(value,"main");
    await page.locator('.dl-card[data-id="6"] .btn-details-toggle').click();
    await shot(value,"details");
    await page.locator('.dl-card[data-id="6"] .btn-details-toggle').click();
    await page.locator('.dl-card[data-id="4"] .btn-details-toggle').click();
    await shot(value,"torrent-details");
    await page.locator('.dl-card[data-id="4"] .btn-details-toggle').click();
    await page.locator("#btn-add").click();await shot(value,"add");
    await page.locator(".advanced-details summary").click();await shot(value,"add-advanced");
    await page.locator(".advanced-details summary").click();await close();
    await page.locator("#quick-source").fill("magnet:?xt=urn:btih:0123456789012345678901234567890123456789");
    await page.locator("#quick-submit").click();
    await page.locator("#probe-torrent-files:not(.hidden)").waitFor();await shot(value,"torrent");await close();
    await page.locator("#quick-source").fill("https://example.org/movie.mp4");await page.locator("#quick-submit").click();
    await page.locator("#probe-details:not(.hidden)").waitFor();await shot(value,"video");await close();
    await page.locator("#btn-grabber").click();await shot(value,"linkgrabber");
    await page.locator("#grab-input").fill("https://example.org/movie.mp4");await page.locator("#grab-run").click();
    await page.locator("#grab-results-area:not(.hidden)").waitFor();await shot(value,"linkgrabber-results");await close();
    await page.locator("#btn-settings").click();
    for(const tab of ["general","connections","speed","scheduler","clipboard","browser","theme","telegram"]){
      await page.locator(`.settings-tab-btn[data-tab="${tab}"]`).click();await shot(value,`settings-${tab}`);
    }
    await close();await page.locator("#btn-about").click();await shot(value,"about");await close();
    await page.locator("#btn-shortcuts").click();await shot(value,"shortcuts");await close();
    await page.evaluate(()=>window.__design.emit("clipboard-url","https://example.org/package.zip"));
    await page.locator("#dlg-clipboard[open]").waitFor();await shot(value,"clipboard");await close();
    await page.locator('.dl-card[data-id="6"]').click({button:"right"});await shot(value,"context-menu");
    await page.locator('[data-action="remove-list"]').click();await page.locator("#dlg-remove[open]").waitFor();await shot(value,"remove");await close();
    await page.locator('.sidebar-item[data-status="failed"]').click();
    await page.locator('.dl-card[data-id="1"] .btn-details-toggle').click();await shot(value,"error");
    await page.locator('.dl-card[data-id="1"] .btn-details-toggle').click();await page.locator('[data-status="all"]').click();
    await page.evaluate(()=>window.__design.failProbe=true);
    await page.locator("#quick-source").fill("https://example.org/unreachable.zip");await page.locator("#quick-submit").click();
    await page.locator("#probe-error:not(.hidden)").waitFor();await shot(value,"probe-error");await close();
    await page.evaluate(()=>window.__design.failProbe=false);
    await page.locator("#downloads-search").fill("no-such-download");await shot(value,"empty-filter");await page.locator("#downloads-search").fill("");
    await page.evaluate(()=>window.__design.empty());await page.locator("#empty-add-btn").waitFor();await shot(value,"empty");
    await page.evaluate(()=>window.__design.reset());await page.locator('.dl-card[data-id="6"]').waitFor();
    await page.locator(".speed-details summary").click();await shot(value,"speed-details");await close();
    await page.locator("#quick-source").fill("invalid text");await page.locator("#quick-submit").click();await shot(value,"validation");await close();await page.locator("#quick-source").fill("");
    for(const width of [900,1024,1920]){
      await page.setViewportSize({width,height:width===1920?1080:800});await shot(value,`width-${width}`);
      check(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true,`${value} no horizontal overflow at ${width}`);
    }
    await page.setViewportSize({width:900,height:560});await shot(value,"minimum-window");
    await page.locator("#btn-add").click();
    await page.locator('#form-add button[type="submit"]').scrollIntoViewIfNeeded();await shot(value,"minimum-window-add");await close();
    await page.setViewportSize({width:1280,height:800});
  }
  // Validate behavior through UI gestures and the command contract.
  await page.locator('[data-status="torrents"]').click();
  check(await page.locator('.dl-card:not(.hidden)').count(),1,"torrent filter");
  await page.locator('.sidebar-item[data-category="Video"]').click();
  check(await page.locator('.dl-card:not(.hidden)').count(),1,"video filter clears prior status filter");
  await page.locator('[data-status="all"]').click();
  await page.locator('.dl-card[data-id="6"] .dl-select-checkbox').click();
  await page.locator('.dl-card[data-id="6"]').focus();await page.keyboard.press("Space");
  await page.waitForFunction(()=>document.querySelector('.dl-card[data-id="6"]').dataset.status==="paused");assertions++;
  await page.locator('.dl-card[data-id="6"]').focus();await page.keyboard.press("Space");
  await page.waitForFunction(()=>document.querySelector('.dl-card[data-id="6"]').dataset.status==="downloading");assertions++;
  await page.locator('.dl-card[data-id="6"]').focus();await page.keyboard.press("Delete");await page.locator("#dlg-remove[open]").waitFor();
  await page.locator("#remove-cancel").click();check(await page.locator('.dl-card').count(),7,"cancel removal keeps queue");
  await page.locator("#pause-all").click();
  await page.waitForFunction(()=>document.getElementById("badge-status-downloading").textContent==="0");assertions++;
  await page.locator("#resume-all").click();
  await page.waitForFunction(()=>document.getElementById("badge-status-downloading").textContent==="5");assertions++;
  await page.evaluate(()=>{window.__design.clipboard="magnet:?xt=urn:btih:abcdef";document.activeElement.blur();});
  await page.keyboard.press("Control+v");await page.locator("#dlg-add[open]").waitFor();
  check(await page.locator("#add-urls").inputValue(),"magnet:?xt=urn:btih:abcdef","Ctrl+V accepts magnets");await close();
  await page.evaluate(()=>{const data=new DataTransfer();data.setData("text/plain","magnet:?xt=urn:btih:drop");window.dispatchEvent(new DragEvent("drop",{dataTransfer:data}));});
  await page.locator("#dlg-add[open]").waitFor();check(await page.locator("#add-urls").inputValue(),"magnet:?xt=urn:btih:drop","drop accepts magnet");await close();
  await page.evaluate(()=>window.__design.emit("tauri://drag-drop",{paths:["C:\\Downloads\\local.torrent"]}));
  await page.locator("#dlg-add[open]").waitFor();check(await page.locator("#add-urls").inputValue(),"C:\\Downloads\\local.torrent","native torrent drop");await close();
  await page.locator("#btn-add").click();await page.locator("#pick-torrent").click();
  check(await page.locator("#add-urls").inputValue(),"C:\\Downloads\\release.torrent","torrent picker");
  await page.locator("#probe-torrent-files:not(.hidden)").waitFor();
  await page.locator("#probe-torrent-files input").nth(1).uncheck();
  await page.locator('#form-add button[type="submit"]').click();await page.waitForFunction(()=>!document.getElementById("dlg-add").open);
  check(await page.evaluate(()=>window.__design.calls.filter(call=>call.command==="add_download").at(-1).args.args.torrent_only_files),[0,2],"selected torrent files reach backend");
  await page.locator("#open-download-folder").click();
  check(await page.evaluate(()=>window.__design.calls.filter(call=>call.command==="open_folder_cmd").at(-1).args.path),"C:\\Downloads","toolbar opens configured folder");
  await page.locator("#btn-theme").click();
  check(await page.evaluate(()=>window.__design.calls.filter(call=>call.command==="set_settings").at(-1).args.settings.theme),"light","theme preference persists");
  await page.evaluate(()=>window.__design.emit("open-settings",null));
  check(await page.locator("#settings-panel").isVisible(),true,"tray Settings opens settings panel");
  await page.locator('.settings-tab-btn[data-tab="general"]').click();
  await page.locator('.settings-tab-btn[data-tab="general"]').focus();await page.keyboard.press("ArrowRight");
  check(await page.locator('.settings-tab-btn[data-tab="connections"]').getAttribute("aria-selected"),"true","settings tabs support keyboard navigation");await close();
  await page.locator("#quick-source").fill("https://example.org/slow.zip");await page.locator("#quick-submit").click();
  await page.waitForFunction(()=>Boolean(window.__design.releaseProbe));
  await page.locator("#add-urls").fill("https://example.org/fast.zip");
  await page.waitForFunction(()=>document.getElementById("add-filename").value==="fast.zip");
  await page.evaluate(()=>window.__design.releaseProbe());
  await page.waitForTimeout(100);
  check(await page.locator("#add-filename").inputValue(),"fast.zip","late probe response cannot overwrite current input");await close();
  await page.evaluate(()=>{window.__design.failAdd=true;window.__design.addError='<img src=x onerror="window.__injected=true">';});
  await page.locator("#quick-source").fill("https://example.org/package.zip");await page.locator("#quick-submit").click();
  await page.locator("#probe-details:not(.hidden)").waitFor();
  await page.locator('#form-add button[type="submit"]').click();
  await page.waitForFunction(()=>document.querySelector(".toast-message")?.textContent.includes("<img"));
  check(await page.locator(".toast img").count(),0,"server error text is never interpreted as markup");
  check(await page.locator("#dlg-add").isVisible(),true,"failed add preserves review dialog");
  await close();await page.evaluate(()=>window.__design.failAdd=false);
  // Untrusted server errors must remain text.
  await page.locator("#quick-source").fill("invalid");await page.locator("#quick-submit").click();
  check(await page.locator(".toast-message").count()>0,true,"input validation feedback");
  check(errors,[],"no uncaught page errors");
  // Show the actual packaged bitmap assets without opening the Windows tray.
  const brandDir=path.resolve(desktop,"../../assets/brand/v3");
  const plate=fs.readFileSync(path.join(brandDir,"jyroxa-tray.png")).toString("base64");
  for (const value of ["light","dark"]) {
    const color=value==="light"?"#1a1a1a":"#f3f3f3",background=value==="light"?"#f3f3f3":"#383a3c";
    const marks=[16,24,32,48,64,128,256].map(size=>`<div><img width="${size}" height="${size}" src="data:image/png;base64,${fs.readFileSync(path.join(brandDir,`jyroxa${value==="dark"?"-dark":""}-${size}.png`)).toString("base64")}"/><p>${size}px</p></div>`).join("");
    await page.setContent(`<body style="margin:0;padding:40px;background:${background};color:${color};font-family:Segoe UI,sans-serif"><h1>Jyroxa</h1><p>Logo and packaged tray icon</p><div style="display:flex;align-items:center;gap:32px;height:320px">${marks}</div><p>Tray bitmap · 16px and 32px</p><img width="16" height="16" src="data:image/png;base64,${plate}"/> <img width="32" height="32" src="data:image/png;base64,${plate}"/><p>The tray menu uses native Windows controls.</p></body>`);
    await shot(value,"logo-and-tray");
  }
  fs.writeFileSync(path.join(reportDir,"verification.json"),JSON.stringify({assertions,screenshots:shots,errors,backend:"deterministic Tauri command fixtures; native executable is built separately",viewport:"1280x800 plus 900, 1024 and 1920"},null,2));
  console.log(`${assertions} interaction assertions passed; ${shots.length} headless screenshots saved in ${reportDir}; no uncaught page errors.`);
} catch (error) {
  console.error(error);
  fs.writeFileSync(path.join(reportDir,"verification-failure.json"), JSON.stringify({message:String(error),errors,screenshots:shots},null,2));
  throw error;
} finally {
  server.kill();
  await browser?.close();
  await browserServer?.kill();
}
