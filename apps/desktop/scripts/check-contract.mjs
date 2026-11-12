#!/usr/bin/env node
/**
 * Frontend ↔ Tauri command contract checker (no dependencies).
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const desktopRoot = path.resolve(__dirname, "..");
const tauriSrc = path.join(desktopRoot, "src-tauri", "src");
const frontSrc = path.join(desktopRoot, "src");

function snakeToCamel(name) {
  return name.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
}

const SKIP_PARAM_TYPES =
  /State\s*<'|AppHandle|WebviewWindow|Window\s*<'|Manager\s*<'/;

function parseCommands(commandsRs) {
  const text = fs.readFileSync(commandsRs, "utf8");
  const commands = new Map();
  const re =
    /#\[tauri::command\]\s*pub\s+(?:async\s+)?fn\s+(\w+)\s*\(([\s\S]*?)\)\s*(?:->|where)/g;
  let m;
  while ((m = re.exec(text)) !== null) {
    const name = m[1];
    const paramsBlock = m[2];
    const params = [];
    for (const line of paramsBlock.split(",")) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith("//")) continue;
      const pm = trimmed.match(/^(\w+)\s*:\s*(.+)$/);
      if (!pm) continue;
      const [, paramName, paramType] = pm;
      if (SKIP_PARAM_TYPES.test(paramType)) continue;
      params.push({ rust: paramName, js: snakeToCamel(paramName) });
    }
    commands.set(name, params);
  }
  return commands;
}

function parseRegistered(libRs) {
  const text = fs.readFileSync(libRs, "utf8");
  const block = text.match(/generate_handler!\[([\s\S]*?)\]/);
  if (!block) throw new Error("invoke_handler block not found in lib.rs");
  const names = [];
  for (const m of block[1].matchAll(/commands::(\w+)/g)) {
    names.push(m[1]);
  }
  return new Set(names);
}

function walkTsFiles(dir, out = []) {
  for (const ent of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, ent.name);
    if (ent.isDirectory()) walkTsFiles(full, out);
    else if (ent.name.endsWith(".ts")) out.push(full);
  }
  return out;
}

function extractBraceObject(text, openBraceIdx) {
  if (text[openBraceIdx] !== "{") return null;
  let depth = 0;
  for (let i = openBraceIdx; i < text.length; i++) {
    const ch = text[i];
    if (ch === "{") depth++;
    else if (ch === "}") {
      depth--;
      if (depth === 0) return text.slice(openBraceIdx, i + 1);
    }
  }
  return null;
}

function keysFromObjectLiteral(objText) {
  const keys = new Set();
  for (const km of objText.matchAll(/(\w+)\s*:/g)) {
    keys.add(km[1]);
  }
  for (const km of objText.matchAll(/(?:^|[,{])\s*(\w+)\s*(?=[,}])/g)) {
    const k = km[1];
    if (k !== "true" && k !== "false" && k !== "null") keys.add(k);
  }
  return [...keys];
}

function parseInvokes(tsFiles) {
  const invokes = [];
  const banned = [];
  const reHead = /invoke\s*(?:<[^>]*>)?\s*\(\s*["']([\w_]+)["']/g;
  for (const file of tsFiles) {
    const text = fs.readFileSync(file, "utf8");
    if (/window\.(confirm|alert|prompt)\s*\(/.test(text)) {
      banned.push(file);
    }
    let m;
    while ((m = reHead.exec(text)) !== null) {
      const cmd = m[1];
      let i = m.index + m[0].length;
      while (i < text.length && /\s/.test(text[i])) i++;
      if (text[i] === ")") {
        invokes.push({ file, cmd, keys: [] });
        continue;
      }
      if (text[i] === ",") {
        i++;
        while (i < text.length && /\s/.test(text[i])) i++;
      }
      if (text[i] === "{") {
        const objText = extractBraceObject(text, i);
        if (objText) {
          invokes.push({ file, cmd, keys: keysFromObjectLiteral(objText) });
        }
      }
    }
  }
  return { invokes, banned };
}

const commandsPath = path.join(tauriSrc, "commands.rs");
const libPath = path.join(tauriSrc, "lib.rs");
const commands = parseCommands(commandsPath);
const registered = parseRegistered(libPath);
const tsFiles = walkTsFiles(frontSrc);
const { invokes, banned } = parseInvokes(tsFiles);

const errors = [];

for (const cmd of registered) {
  if (!commands.has(cmd)) {
    errors.push(`Registered command not defined: ${cmd}`);
  }
}

for (const [name] of commands) {
  if (!registered.has(name)) {
    errors.push(`Command defined but not registered: ${name}`);
  }
}

for (const inv of invokes) {
  if (!registered.has(inv.cmd)) {
    errors.push(`${path.relative(desktopRoot, inv.file)}: unknown command "${inv.cmd}"`);
    continue;
  }
  const expected = commands.get(inv.cmd) ?? [];
  const expectedKeys = new Set(expected.map((p) => p.js));
  for (const key of inv.keys) {
    if (!expectedKeys.has(key)) {
      errors.push(
        `${path.relative(desktopRoot, inv.file)}: invoke("${inv.cmd}") unexpected key "${key}" (expected: ${[...expectedKeys].join(", ") || "none"})`
      );
    }
  }
  for (const exp of expected) {
    if (!inv.keys.includes(exp.js)) {
      errors.push(
        `${path.relative(desktopRoot, inv.file)}: invoke("${inv.cmd}") missing key "${exp.js}"`
      );
    }
  }
}

// Plugin JS APIs are checked against the capability file: a plugin's
// `:default` set does not always grant what the frontend calls (the
// clipboard-manager default grants nothing, so copy/paste silently failed).
const PLUGIN_PERMISSIONS = {
  "@tauri-apps/plugin-clipboard-manager": {
    writeText: ["clipboard-manager:allow-write-text"],
    readText: ["clipboard-manager:allow-read-text"],
  },
  "@tauri-apps/plugin-dialog": {
    open: ["dialog:allow-open", "dialog:default"],
    save: ["dialog:allow-save", "dialog:default"],
    message: ["dialog:allow-message", "dialog:default"],
    ask: ["dialog:allow-message", "dialog:allow-ask", "dialog:default"],
    confirm: ["dialog:allow-message", "dialog:allow-confirm", "dialog:default"],
  },
  "@tauri-apps/plugin-opener": {
    openUrl: ["opener:allow-open-url", "opener:default"],
    revealItemInDir: ["opener:allow-reveal-item-in-dir", "opener:default"],
    openPath: ["opener:allow-open-path"],
  },
  "@tauri-apps/plugin-notification": {
    sendNotification: ["notification:allow-notify", "notification:default"],
  },
};

const capabilityPath = path.join(desktopRoot, "src-tauri", "capabilities", "default.json");
const granted = new Set(
  (JSON.parse(fs.readFileSync(capabilityPath, "utf8")).permissions ?? []).map((p) =>
    typeof p === "string" ? p : p.identifier
  )
);
for (const file of tsFiles) {
  const text = fs.readFileSync(file, "utf8");
  for (const m of text.matchAll(/import\s*\{([^}]*)\}\s*from\s*["'](@tauri-apps\/plugin-[\w-]+)["']/g)) {
    const table = PLUGIN_PERMISSIONS[m[2]];
    for (const raw of m[1].split(",")) {
      const fn = raw.trim().split(/\s+as\s+/)[0];
      if (!fn) continue;
      const needs = table?.[fn];
      if (!needs) {
        errors.push(`${path.relative(desktopRoot, file)}: ${m[2]}.${fn} has no permission mapping in check-contract.mjs`);
      } else if (!needs.some((p) => granted.has(p))) {
        errors.push(`${path.relative(desktopRoot, file)}: ${m[2]}.${fn} needs one of [${needs.join(", ")}] in capabilities/default.json`);
      }
    }
  }
}

for (const f of banned) {
  errors.push(`${path.relative(desktopRoot, f)}: uses window.confirm/alert/prompt`);
}

if (errors.length) {
  console.error("Contract check failed:\n");
  for (const e of errors) console.error(`  - ${e}`);
  process.exit(1);
}

console.log(
  `Contract OK (${commands.size} commands, ${invokes.length} invoke sites, ${tsFiles.length} TS files)`
);
