import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const desktop = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const source = path.join(desktop, "node_modules/@fluentui/svg-icons/icons");
const names = {
  video: "video_clip", audio: "music_note_2", music: "music_note_2", archive: "archive",
  image: "image", document: "document_text", fileText: "document_text", executable: "app_generic",
  programs: "app_generic", code: "code", other: "document", file: "document",
  play: "play", pause: "pause", cancel: "dismiss", trash: "delete", retry: "arrow_clockwise",
  folder: "folder", copy: "copy", chevronDown: "chevron_down", chevronUp: "chevron_up",
  chevronRight: "chevron_right", plus: "add", download: "arrow_download", settings: "settings",
  link: "link", sun: "weather_sunny", moon: "weather_moon", check: "checkmark",
  checkCircle: "checkmark_circle", alertCircle: "error_circle", alertTriangle: "warning",
  info: "info", helpCircle: "question_circle", package: "box", filter: "filter", sort: "arrow_sort",
  arrowUp: "arrow_up", arrowDown: "arrow_down", refresh: "arrow_sync", search: "search",
  externalLink: "open", zap: "flash", shield: "shield", server: "server", edit: "edit",
  clock: "clock", calendar: "calendar", wifi: "wifi_1", gauge: "top_speed", clipboard: "clipboard",
  layers: "list", x: "dismiss", spinner: "spinner_ios", network: "share",
};
const definitions = Object.entries(names).map(([name, file]) => {
  const svg = fs.readFileSync(path.join(source, `${file}_20_regular.svg`), "utf8");
  const body = svg.replace(/^<svg[^>]*>/, "").replace(/<\/svg>\s*$/, "").replaceAll(/fill="[^"]*"/g, 'fill="currentColor"');
  return `  ${name}: (size = 20) => svg(${JSON.stringify(body)}, size),`;
});
fs.writeFileSync(path.join(desktop, "src/icons.ts"), `// Microsoft Fluent System Icons, regular 20px. MIT license: docs/FLUENT-ICONS-LICENSE.txt.
function svg(paths: string, size: number): string {
  return \`<svg width="\${size}" height="\${size}" viewBox="0 0 20 20" fill="currentColor" class="svg-icon" aria-hidden="true">\${paths}</svg>\`;
}
export const icons = {
${definitions.join("\n")}
};
`);
// Replace static shell icons with the same catalog as the dynamic views.
const htmlFile = path.join(desktop, "index.html");
let html = fs.readFileSync(htmlFile, "utf8");
const staticIcons = { "btn-add": "plus", "btn-grabber": "link", "btn-settings": "settings", "btn-shortcuts": "helpCircle", "btn-about": "info", "btn-sort-dir": "arrowDown", "sidebar-close": "x", "add-close-btn": "x", "clip-close-btn": "x", "shortcuts-close-btn": "x", "about-close-btn": "x", "remove-close-btn": "x", "btn-pick-folder": "folder", "bulk-pause": "pause", "bulk-resume": "play", "bulk-cancel": "cancel", "bulk-remove": "trash", "bulk-open": "folder", "bulk-deselect": "x" };
const render = name => {
  const file = names[name];
  return fs.readFileSync(path.join(source, `${file}_20_regular.svg`), "utf8").replace(/<svg[^>]*>/, '<svg width="20" height="20" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">').replaceAll(/fill="#[^"]*"/g, 'fill="currentColor"');
};
for (const [id, name] of Object.entries(staticIcons)) {
  const expression = new RegExp(`(<button[^>]*id="${id}"[^>]*>\\s*)<svg[\\s\\S]*?<\\/svg>`);
  html = html.replace(expression, (_, start) => start + render(name));
}
html = html.replace(/(<span class="icon-sun">\s*)<svg[\s\S]*?<\/svg>/, (_, start) => start + render("sun"));
html = html.replace(/(<span class="icon-moon">\s*)<svg[\s\S]*?<\/svg>/, (_, start) => start + render("moon"));
html = html.replace(/(<li data-action="([^"]+)"[^>]*>\s*)<svg[\s\S]*?<\/svg>/g, (_, start, action) => start + render(({ "open-file": "play", "open-folder": "folder", pause: "pause", resume: "play", retry: "retry", cancel: "cancel", "copy-url": "copy", "copy-path": "copy", "remove-list": "trash", "remove-file": "trash" })[action]));
fs.writeFileSync(htmlFile, html);
console.log(`Updated ${definitions.length} Fluent icons and static shell actions.`);
