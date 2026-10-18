#!/usr/bin/env node
// sync-codex.mjs — Claude Code の設定（正本）から Codex 用の生成物を冪等に作る
//
// 生成物:
//   1) CODEX_HOME/AGENTS.md            ← codex/AGENTS.header.md ＋ CLAUDE.md
//   2) AGENTS_SKILLS/<name>            ← CLAUDE_DIR/skills/<name> へのジャンクション
//   3) CODEX_HOME/agents/<name>.toml   ← CLAUDE_DIR/agents/*.md
//   4) CODEX_HOME/hooks.json           ← CLAUDE_DIR/settings.json の hooks
// 引数: なし=同期 / --check=書かずに差分があれば exit 1 / --quiet=成功時 stdout 無し・致命的エラーでも exit 0 / --force=人手ファイルも上書き(AGENTS.md のみ)
// 方針: 人が書いたファイル（自動生成の目印が無い）は上書きしない。内容が同じなら書かない。
//   ジャンクションの削除はリンク自体だけ（リンク先の中身は絶対に消さない）。秘密は扱わない。
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const args = new Set(process.argv.slice(2));
const CHECK = args.has('--check');
const QUIET = args.has('--quiet');
const FORCE = args.has('--force');

const envDir = (n) => { const v = process.env[n]; return v && v.trim() ? v.trim() : null; };
const HOME = os.homedir();
const CLAUDE_DIR = envDir('CLAUDE_CONFIG_DIR') ?? path.join(HOME, '.claude');
const CODEX_HOME = envDir('CODEX_HOME') ?? path.join(HOME, '.codex');
const AGENTS_SKILLS = envDir('CODEX_SYNC_AGENTS_SKILLS_DIR') ?? path.join(HOME, '.agents', 'skills');

const MD_MARK = '<!-- 自動生成: ~/.claude/scripts/sync-codex.mjs';
const TOML_MARK = '# 自動生成: ~/.claude/scripts/sync-codex.mjs';
const HOOKS_DESC = '自動生成: ~/.claude/scripts/sync-codex.mjs（元: ~/.claude/settings.json の hooks）。直接編集しない。';
const CODEX_EVENTS = new Set(['SessionStart', 'SessionEnd', 'UserPromptSubmit', 'PreToolUse', 'PermissionRequest',
  'PostToolUse', 'PreCompact', 'PostCompact', 'SubagentStart', 'SubagentStop', 'Stop', 'Interrupt']);
const ADAPTED_EVENTS = new Set(['Stop', 'SubagentStop']);
const MAX_AGENTS_BYTES = 32768;

let changes = 0; // created/updated/removed の件数（--check の判定用）
const out = (s) => { if (!QUIET) console.log(s); };
const warn = (s) => console.error(`[sync-codex] 警告: ${s}`);
const skipped = (s) => { const line = `skipped(${s})`; if (QUIET) console.error(`[sync-codex] ${line}`); else console.log(line); };
const report = (kind, p) => { changes++; out(`${kind} ${p}${CHECK ? '  (--check: 未反映)' : ''}`); };

function readText(p) { return fs.readFileSync(p, 'utf8').replace(/^\uFEFF/, '').replace(/\r\n/g, '\n'); }
function exists(p) { try { fs.lstatSync(p); return true; } catch { return false; } }
function ensureDir(d) { if (!CHECK) fs.mkdirSync(d, { recursive: true }); }

// 生成ファイルを書く。marker で始まらない既存ファイルは force でない限り触らない
function writeGenerated(file, content, marker, label, force = false) {
  let old = null;
  if (exists(file)) {
    try { old = fs.readFileSync(file, 'utf8').replace(/^\uFEFF/, '').replace(/\r\n/g, '\n'); } catch { old = null; }
    if (old != null && old === content) return;
    if (old == null || (!old.startsWith(marker) && !force)) { skipped(`${label}: 自動生成の目印が無い既存ファイルは上書きしない: ${file}`); return; }
  }
  report(old == null ? 'created' : 'updated', file);
  if (!CHECK) { ensureDir(path.dirname(file)); fs.writeFileSync(file, content, 'utf8'); }
}

// ---------- 設定 ----------
function loadConfig() {
  const cfg = { exclude: [], modelMap: {} };
  const p = path.join(CLAUDE_DIR, 'codex', 'sync-config.json');
  if (!exists(p)) return cfg;
  try {
    const j = JSON.parse(readText(p));
    if (Array.isArray(j?.skills?.exclude)) cfg.exclude = j.skills.exclude.map(String);
    if (j?.agents?.modelMap && typeof j.agents.modelMap === 'object') cfg.modelMap = j.agents.modelMap;
  } catch (e) { warn(`sync-config.json を読めません（既定値で続行）: ${e.message}`); }
  return cfg;
}

// ---------- 1) AGENTS.md ----------
function syncAgentsMd() {
  const headerP = path.join(CLAUDE_DIR, 'codex', 'AGENTS.header.md');
  const claudeP = path.join(CLAUDE_DIR, 'CLAUDE.md');
  if (!exists(headerP) || !exists(claudeP)) { skipped(`AGENTS.md: 入力が無い（${headerP} / ${claudeP}）`); return; }
  const content = `${MD_MARK}。直接編集しない。編集元: ~/.claude/codex/AGENTS.header.md（前半）と ~/.claude/CLAUDE.md（後半） -->\n\n`
    + readText(headerP)
    + '\n\n---\n\n# 以下は ~/.claude/CLAUDE.md の本文（自動転記・Codex では上の読み替えを適用）\n\n'
    + readText(claudeP);
  const bytes = Buffer.byteLength(content, 'utf8');
  if (bytes > MAX_AGENTS_BYTES) warn(`AGENTS.md が ${bytes} バイトで project_doc_max_bytes 既定(${MAX_AGENTS_BYTES})を超えています。後半が切り捨てられます`);
  writeGenerated(path.join(CODEX_HOME, 'AGENTS.md'), content, MD_MARK, 'AGENTS.md', FORCE);
}

// ---------- 2) スキルのジャンクション ----------
function normLink(p) {
  let s = String(p).replace(/^\\\\\?\\/, '');
  try { s = path.resolve(s); } catch { /* そのまま */ }
  return s.replace(/[\\/]+$/, '').toLowerCase();
}
function isLink(p) { try { return fs.lstatSync(p).isSymbolicLink(); } catch { return false; } }

function eligibleSkills(cfg) {
  const root = path.join(CLAUDE_DIR, 'skills');
  const names = [];
  if (!exists(root)) return names;
  for (const e of fs.readdirSync(root, { withFileTypes: true })) {
    if (e.name.startsWith('.') || cfg.exclude.includes(e.name)) continue;
    const dir = path.join(root, e.name);
    let isDir = e.isDirectory();
    if (!isDir) { try { isDir = fs.statSync(dir).isDirectory(); } catch { isDir = false; } }
    if (isDir && exists(path.join(dir, 'SKILL.md'))) names.push(e.name);
  }
  return names;
}

function syncSkills(cfg) {
  const skillsRoot = path.join(CLAUDE_DIR, 'skills');
  const names = eligibleSkills(cfg);
  if (!exists(AGENTS_SKILLS) && names.length) ensureDir(AGENTS_SKILLS);
  for (const name of names) {
    const target = path.join(skillsRoot, name);
    const link = path.join(AGENTS_SKILLS, name);
    if (exists(link)) {
      if (isLink(link)) {
        let dest = null;
        try { dest = fs.readlinkSync(link); } catch { /* 読めない */ }
        if (dest != null && normLink(dest) === normLink(target)) continue;
        skipped(`conflict: ${link} は別の場所を指すリンク（${dest}）`);
      } else skipped(`実体あり。触らない: ${link}`);
      continue;
    }
    report('created', `${link} -> ${target}`);
    if (!CHECK) {
      try { fs.symlinkSync(target, link, 'junction'); } catch (e) { warn(`ジャンクション作成に失敗 ${link}: ${e.message}`); }
    }
  }
  // 掃除: CLAUDE_DIR/skills 配下を指す、壊れた/除外された/SKILL.md の無いリンクだけ消す
  if (!exists(AGENTS_SKILLS)) return;
  const rootN = normLink(skillsRoot);
  for (const e of fs.readdirSync(AGENTS_SKILLS)) {
    const link = path.join(AGENTS_SKILLS, e);
    if (!isLink(link)) continue;
    let dest; try { dest = fs.readlinkSync(link); } catch { continue; }
    const destN = normLink(dest);
    if (!destN.startsWith(rootN + path.sep)) continue; // 管轄外のリンクには触れない
    const base = path.basename(destN);
    const stale = !exists(dest) || cfg.exclude.some((x) => x.toLowerCase() === base) || !exists(path.join(dest, 'SKILL.md')) || base.startsWith('.');
    if (!stale) continue;
    report('removed', link);
    if (!CHECK) {
      // リンク自体だけを消す（recursive は使わない。junction は rmdir/unlink でリンクのみ消える）
      try { fs.unlinkSync(link); } catch { try { fs.rmdirSync(link); } catch (err) { warn(`リンク削除に失敗 ${link}: ${err.message}`); } }
    }
  }
}

// ---------- 3) エージェント TOML ----------
function stripQuotes(v) {
  if (v.length >= 2 && ((v[0] === '"' && v.at(-1) === '"') || (v[0] === "'" && v.at(-1) === "'"))) {
    const q = v[0]; const inner = v.slice(1, -1);
    return q === '"' ? inner.replace(/\\(["\\])/g, '$1') : inner;
  }
  return v;
}

// 小さな YAML サブセットの frontmatter 解析
function parseFrontmatter(text) {
  const m = text.match(/^---\n([\s\S]*?)\n---[ \t]*(?:\n|$)/);
  if (!m) return { fm: {}, body: text };
  const lines = m[1].split('\n');
  const fm = {};
  for (let i = 0; i < lines.length; i++) {
    const kv = lines[i].match(/^([A-Za-z_][\w-]*):[ \t]*(.*)$/);
    if (!kv) continue;
    const key = kv[1]; const rest = kv[2].trim();
    const block = rest.match(/^([>|])([+-]?)$/);
    if (block) {
      const buf = [];
      while (i + 1 < lines.length && (/^[ \t]/.test(lines[i + 1]) || lines[i + 1].trim() === '')) buf.push(lines[++i]);
      const indent = Math.min(...buf.filter((l) => l.trim()).map((l) => l.match(/^[ \t]*/)[0].length), Infinity);
      const ls = buf.map((l) => (Number.isFinite(indent) ? l.slice(indent) : l).replace(/[ \t]+$/, ''));
      while (ls.length && ls.at(-1) === '') ls.pop();
      fm[key] = (block[1] === '>' ? ls.join(' ') : ls.join('\n')).trim();
    } else fm[key] = stripQuotes(rest);
  }
  return { fm, body: text.slice(m[0].length).replace(/^\n+/, '') };
}

const uesc = (c) => `\\u${c.toString(16).toUpperCase().padStart(4, '0')}`;
function tomlBasic(s) {
  let r = '';
  for (const ch of String(s)) {
    const c = ch.codePointAt(0);
    if (ch === '\\') r += '\\\\';
    else if (ch === '"') r += '\\"';
    else if (ch === '\n') r += '\\n';
    else if (ch === '\t') r += '\\t';
    else if (ch === '\r') r += '\\r';
    else if (c < 0x20 || c === 0x7f) r += uesc(c);
    else r += ch;
  }
  return `"${r}"`;
}
// 複数行基本文字列の本体（\ をエスケープし、" の3連続以上は全部エスケープして区切りを壊さない）
function tomlMultilineBody(s) {
  let r = '';
  for (const ch of String(s).replace(/\r\n/g, '\n').replace(/\r/g, '\n')) {
    const c = ch.codePointAt(0);
    if (ch === '\\') r += '\\\\';
    else if (ch === '\n' || ch === '\t') r += ch;
    else if (c < 0x20 || c === 0x7f) r += uesc(c);
    else r += ch;
  }
  return r.replace(/"{3,}/g, (m) => '\\"'.repeat(m.length));
}

const WRITE_TOOLS = ['Write', 'Edit', 'MultiEdit', 'NotebookEdit'];
const splitList = (v) => String(v ?? '').split(/[,\s]+/).map((x) => x.trim()).filter(Boolean);

function agentToml(file, text, modelMap) {
  const { fm, body } = parseFrontmatter(text);
  const name = fm.name || path.basename(file, '.md');
  const notes = [`（この指示は Claude Code 用のサブエージェント定義 ~/.claude/agents/${path.basename(file)} を Codex 用に変換したもの。ツール名は ~/.codex/AGENTS.md の読み替え表に従う）`];
  const denied = splitList(fm.disallowedTools).filter((t) => WRITE_TOOLS.includes(t));
  if (denied.length) notes.push(`Claude 版ではファイル書き込みツール（${denied.join(' / ')}）を機構で無効化していた。Codex のカスタムエージェントはサンドボックスを個別に指定できないため、apply_patch・シェル経由を含め一切のファイル書き込みをしないこと。機構で塞ぐには親セッションを read-only サンドボックスで起動する。`);
  const allowed = splitList(fm.tools);
  if (allowed.length) notes.push(`Claude 版で使えたツール: ${allowed.join(', ')}（これ以外は使わない）`);
  const lines = [`${TOML_MARK}（元: ~/.claude/agents/${path.basename(file)}）。直接編集しない。`, `name = ${tomlBasic(name)}`, `description = ${tomlBasic(fm.description ?? '')}`];
  const mm = fm.model ? modelMap[fm.model] : null;
  if (mm && typeof mm === 'object') {
    for (const [k, v] of Object.entries(mm)) {
      if (v == null || v === '') continue;
      if (!/^[A-Za-z0-9_-]+$/.test(k)) { warn(`modelMap のキーが不正のため無視: ${k}`); continue; }
      lines.push(`${k} = ${tomlBasic(v)}`);
    }
  }
  lines.push('developer_instructions = """', tomlMultilineBody(`${notes.join('\n')}\n${body.replace(/\s+$/, '')}`), '"""');
  return { name, toml: lines.join('\n') + '\n' };
}

function syncAgents(cfg) {
  const srcDir = path.join(CLAUDE_DIR, 'agents');
  const dstDir = path.join(CODEX_HOME, 'agents');
  const wanted = new Set();
  if (exists(srcDir)) {
    for (const f of fs.readdirSync(srcDir).filter((x) => x.toLowerCase().endsWith('.md'))) {
      let gen;
      const stem = path.basename(f, '.md');
      wanted.add(stem + '.toml'); // 変換に失敗しても既存の生成物は掃除対象にしない
      try { gen = agentToml(f, readText(path.join(srcDir, f)), cfg.modelMap); } catch (e) { warn(`${f} を変換できません（既存の toml は保持）: ${e.message}`); continue; }
      writeGenerated(path.join(dstDir, stem + '.toml'), gen.toml, TOML_MARK, `agents/${stem}.toml`);
    }
  }
  // 掃除: 目印つきで元 md が無くなった toml
  if (!exists(dstDir)) return;
  for (const f of fs.readdirSync(dstDir).filter((x) => x.endsWith('.toml'))) {
    if (wanted.has(f)) continue;
    const p = path.join(dstDir, f);
    let head = ''; try { head = fs.readFileSync(p, 'utf8').replace(/^\uFEFF/, ''); } catch { continue; }
    if (!head.startsWith(TOML_MARK)) continue;
    report('removed', p);
    if (!CHECK) fs.unlinkSync(p);
  }
}

// ---------- 4) hooks.json ----------
function quoteWinArg(a) {
  const s = String(a);
  if (!(s === '' || /[\s"&|<>^()%!]/.test(s))) return s;
  // Windows の引数規則: 引用符の直前（と閉じ引用符の直前）のバックスラッシュ列は二重化し、内部の " は \" にする
  let r = ''; let bs = 0;
  for (const ch of s) {
    if (ch === '\\') bs++;
    else if (ch === '"') { r += '\\'.repeat(bs * 2 + 1) + '"'; bs = 0; }
    else { r += '\\'.repeat(bs) + ch; bs = 0; }
  }
  return `"${r}${'\\'.repeat(bs * 2)}"`;
}
function joinCommand(h) {
  const base = String(h.command ?? '');
  // args なしの文字列 command は完成したコマンドラインなのでそのまま。exec 形式は実行ファイルも引数も引用する
  return Array.isArray(h.args) ? [quoteWinArg(base), ...h.args.map(quoteWinArg)].join(' ') : base;
}

function buildHooks() {
  const sp = path.join(CLAUDE_DIR, 'settings.json');
  if (!exists(sp)) { skipped(`hooks.json: ${sp} が無い`); return null; }
  let hooks;
  try { hooks = JSON.parse(readText(sp)).hooks ?? {}; } catch (e) { warn(`settings.json を読めません: ${e.message}`); return null; }
  const adapter = path.join(CLAUDE_DIR, 'scripts', 'codex-hook-adapter.mjs').replace(/\\/g, '/');
  const result = {};
  for (const [event, groups] of Object.entries(hooks)) {
    if (!CODEX_EVENTS.has(event)) { warn(`Codex に無いイベントを除外: ${event}`); continue; }
    const outGroups = [];
    for (const g of Array.isArray(groups) ? groups : []) {
      const hs = [];
      for (const h of Array.isArray(g?.hooks) ? g.hooks : []) {
        if (h?.type !== 'command') { warn(`${event}: type=${h?.type} のハンドラーを除外（command のみ対応）`); continue; }
        const o = { type: 'command' };
        let cmd = joinCommand(h);
        if (ADAPTED_EVENTS.has(event)) cmd = `node "${adapter}" ${cmd}`;
        o.command = cmd;
        if (h.timeout != null) o.timeout = h.timeout;
        if (h.statusMessage != null) o.statusMessage = h.statusMessage;
        for (const k of Object.keys(h)) if (!['type', 'command', 'args', 'timeout', 'statusMessage'].includes(k)) warn(`${event}: 未対応キー ${k} を除外`);
        hs.push(o);
      }
      if (!hs.length) continue;
      const og = {};
      if (g.matcher != null) og.matcher = g.matcher;
      og.hooks = hs;
      outGroups.push(og);
    }
    if (outGroups.length) result[event] = outGroups;
  }
  return { description: HOOKS_DESC, hooks: result };
}

function syncHooks() {
  const obj = buildHooks();
  if (!obj) return;
  const file = path.join(CODEX_HOME, 'hooks.json');
  const content = JSON.stringify(obj, null, 2) + '\n';
  let old = null;
  if (exists(file)) {
    try { old = fs.readFileSync(file, 'utf8').replace(/^\uFEFF/, '').replace(/\r\n/g, '\n'); } catch { /* 読めない */ }
    if (old === content) return;
    let desc = null; try { desc = JSON.parse(old)?.description; } catch { /* 壊れ */ }
    if (typeof desc !== 'string' || !desc.startsWith('自動生成: ~/.claude/scripts/sync-codex.mjs')) {
      skipped(`hooks.json: 自動生成の目印が無い既存ファイルは上書きしない: ${file}`); return;
    }
  }
  report(old == null ? 'created' : 'updated', file);
  if (!CHECK) { ensureDir(CODEX_HOME); fs.writeFileSync(file, content, 'utf8'); }
  console.error('[sync-codex] hooks.json が変わりました。Codex CLI の /hooks で再信頼が必要です。');
}

function main() {
  const cfg = loadConfig();
  const steps = [['AGENTS.md', syncAgentsMd], ['skills', () => syncSkills(cfg)], ['agents', () => syncAgents(cfg)], ['hooks', syncHooks]];
  for (const [label, fn] of steps) {
    try { fn(); } catch (e) {
      console.error(`[sync-codex] ${label} でエラー: ${e?.stack ?? e}`);
      if (!QUIET) process.exitCode = 2;
    }
  }
  if (CHECK) {
    if (changes > 0) { (QUIET ? console.error : console.log)(`同期が必要: ${changes} 件の差分`); process.exit(1); }
    out('差分なし');
  }
}

try { main(); } catch (e) {
  console.error(`[sync-codex] 致命的エラー: ${e?.stack ?? e}`);
  process.exit(QUIET ? 0 : 2);
}
if (QUIET && !CHECK) process.exit(0);
