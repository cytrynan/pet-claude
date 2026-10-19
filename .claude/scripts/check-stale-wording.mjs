#!/usr/bin/env node
// check-stale-wording.mjs — Stop hook: このターンで消した言い回しが、触った文書（＋触った／最新の .steering/<作業>/）の
// 「触らなかった行」に残っていたら、直すか履歴と判定するよう Claude に返す。
// CLAUDE.md「commit 前に旧値で git grep して記録文書を追随」を文言でなく機構で支える（文言だけでは想起されずに破られた）。
//
// 契約（公式 https://code.claude.com/docs/en/hooks 「Stop decision control」2026-10-19 取得）:
//   stdin  = Stop の JSON（transcript_path / cwd / stop_hook_active）。当ターンのツール呼び出しは来ない＝transcript を読む
//   stdout = 該当あり かつ stop_hook_active=false → {hookSpecificOutput:{hookEventName:"Stop", additionalContext}}
//            （block ではなく feedback。会話は続き、ループ保護は stop_hook_active と連続8回上限）
//            stop_hook_active=true → 何も出さない（1ターン1回まで）
//   exit 0 固定。壊れた入力・git 失敗はフェイルオープン（ただしログに error を残す＝「測れていない」を見える化）
//
// 基準（ターン開始時点の内容）: リポごとに HEAD@{ターン開始時刻}（reflog）。そのターンで新規に Write したファイルは
// 最初の Write の内容。→ Edit・Python 置換・sed のどれで書き換えても同じく比較できる。
// 対象リポ: そのターンで触ったパス（Write/Edit の file_path・Bash/PowerShell のコマンド中のパス・cwd）の git ルートのうち
// .steering/ を持つもの（テンプレ由来のプロジェクト）。
// ログ: <このスクリプトの1つ上>/logs/stale-wording-fires.jsonl（対象リポに触ったターンだけ1行）。
// 故障注入: tests/test-check-stale-wording.mjs（週次ハーネス verify-hooks.mjs が自動で拾う）。
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { staleSegments, isNarrowTarget, newestSteeringDir } from './stale-wording-core.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const LOG = process.env.STALE_WORDING_LOG ?? path.join(HERE, '..', 'logs', 'stale-wording-fires.jsonl');
const TEXT = /\.(md|ts|tsx|mjs|js|py|json|html|txt|ya?ml)$/i;
const MAX_SHOW = 8;

const log = (o) => {
  try { fs.mkdirSync(path.dirname(LOG), { recursive: true }); fs.appendFileSync(LOG, JSON.stringify({ at: new Date().toISOString(), ...o }) + '\n'); } catch {}
};
const git = (repo, ...a) => execFileSync('git', ['-C', repo, ...a], { encoding: 'utf8', maxBuffer: 1 << 26, stdio: ['ignore', 'pipe', 'ignore'] });

function readTurn(tp) {
  // 最後の「本物のユーザー発話」（tool_result でない）以降が当ターン
  const objs = fs.readFileSync(tp, 'utf8').split('\n').map(l => { try { return JSON.parse(l); } catch { return null; } });
  let start = -1, startAt = null;
  objs.forEach((o, i) => {
    if (o?.type !== 'user') return;
    const c = o.message?.content;
    if (typeof c === 'string' || (Array.isArray(c) && c.some(x => x?.type === 'text'))) { start = i; startAt = o.timestamp; }
  });
  const uses = [];
  for (const o of objs.slice(start + 1)) {
    for (const it of Array.isArray(o?.message?.content) ? o.message.content : []) if (it?.type === 'tool_use') uses.push(it);
  }
  return { startAt, uses };
}

function candidateRepos(uses, cwd) {
  const dirs = new Set([cwd]);
  const PATH_RE = /(?:[A-Za-z]:[\\/]|\/[a-zA-Z]\/)[^\s"'`;|&<>()]+/g;
  for (const u of uses) {
    const fp = u.input?.file_path; if (fp) dirs.add(path.dirname(fp));
    const cmd = u.input?.command; if (typeof cmd !== 'string') continue;
    for (let m of cmd.match(PATH_RE) ?? []) {
      m = m.replace(/^\/([a-zA-Z])\//, '$1:/');
      while (m && !fs.existsSync(m)) { const p = path.dirname(m); if (p === m) break; m = p; }
      try { if (m) dirs.add(fs.statSync(m).isDirectory() ? m : path.dirname(m)); } catch {}
    }
  }
  const repos = new Set();
  for (const d of dirs) {
    try { const top = git(d, 'rev-parse', '--show-toplevel').trim(); if (fs.existsSync(path.join(top, '.steering'))) repos.add(path.resolve(top)); } catch {}
  }
  return [...repos];
}

function checkRepo(repo, startAt, uses) {
  let base = 'HEAD';
  try { if (startAt) base = git(repo, 'rev-parse', '--verify', '-q', `HEAD@{${startAt}}`).trim(); } catch {}
  const firstWrite = new Map(); // repo 相対パス → そのターン最初の Write 内容
  for (const u of uses) {
    if (u.name !== 'Write' || !u.input?.file_path) continue;
    const rel = path.relative(repo, path.resolve(u.input.file_path)).replace(/\\/g, '/');
    if (!rel.startsWith('..') && !firstWrite.has(rel)) firstWrite.set(rel, u.input.content ?? '');
  }
  const changes = [];
  const touched = new Set();
  const add = (rel, st) => {
    const m = rel.match(/^\.steering\/([^/]+)\//); if (m) touched.add(m[1]);
    if (!TEXT.test(rel)) return;
    let before = null;
    if (st !== 'A' && st !== '?') { try { before = git(repo, 'show', `${base}:${rel}`); } catch {} }
    if (before == null && firstWrite.has(rel)) before = firstWrite.get(rel);
    const abs = path.join(repo, rel);
    changes.push({ path: rel, before, after: fs.existsSync(abs) ? fs.readFileSync(abs, 'utf8') : null });
  };
  for (const l of git(repo, 'diff', '--name-status', '--no-renames', base).split('\n').filter(Boolean)) {
    const [st, rel] = l.split('\t'); add(rel, st[0]);
  }
  for (const rel of git(repo, 'ls-files', '--others', '--exclude-standard').split('\n').filter(Boolean)) add(rel, '?');
  const changedMap = new Map(changes.map(c => [c.path, c]));
  const files = git(repo, 'ls-files', '--cached', '--others', '--exclude-standard').split('\n').filter(Boolean);
  const newest = newestSteeringDir(files); if (newest) touched.add(newest);
  const targets = [];
  for (const rel of files) {
    if (!isNarrowTarget(rel, touched, changedMap)) continue;
    const abs = path.join(repo, rel);
    if (!fs.existsSync(abs)) continue;
    const c = changedMap.get(rel);
    targets.push({ path: rel, text: fs.readFileSync(abs, 'utf8'), before: c ? (c.before ?? '') : null });
  }
  return staleSegments(changes, targets);
}

function main() {
  let input;
  try { input = JSON.parse(fs.readFileSync(0, 'utf8')); } catch { return; }
  if (input.stop_hook_active) return;
  if (!input.transcript_path || !fs.existsSync(input.transcript_path)) return;
  const t0 = Date.now();
  const { startAt, uses } = readTurn(input.transcript_path);
  const repos = candidateRepos(uses, input.cwd ?? process.cwd());
  if (!repos.length) return; // 対象外のターンは記録もしない
  const all = [];
  const errors = [];
  for (const repo of repos) {
    try { for (const x of checkRepo(repo, startAt, uses)) all.push({ repo, ...x }); }
    catch (e) { errors.push({ repo, error: String(e).slice(0, 200) }); }
  }
  // 試用期間の判定（直した件数）用に指摘の中身も残す。後で「その行がまだあるか」を照合すれば直したかが分かる
  log({ session: input.session_id, ms: Date.now() - t0, repos: repos.length, fired: all.length > 0, findings: all.length,
    ...(errors.length ? { errors } : {}),
    items: all.slice(0, 20).map(x => ({ repo: x.repo, seg: x.segment, hits: x.hits.slice(0, 3).map(h => ({ path: h.path, line: h.line, text: h.text })) })) });
  if (!all.length) return;
  const lines = all.slice(0, MAX_SHOW).map(x => {
    const h = x.hits.slice(0, 2).map(h => `${h.path}:${h.line}`).join(', ');
    return `- 「${x.segment}」（このターンで ${x.from} から消えた）→ まだ残る: ${h}${x.hits.length > 2 ? ` ほか${x.hits.length - 2}行` : ''}`;
  });
  if (all.length > MAX_SHOW) lines.push(`- ほか ${all.length - MAX_SHOW} 件`);
  const msg = [
    '[stale-wording] このターンで消した言い回しが、触った文書の触っていない行に残っています（CLAUDE.md「commit 前に旧値で git grep して記録文書を追随」の機械検査）。',
    ...lines,
    '各行について、古い記述なら直す／履歴・対照表として正しいなら「履歴」と1行で述べる。どちらか済めば終えてよい。',
  ].join('\n');
  process.stdout.write(JSON.stringify({ hookSpecificOutput: { hookEventName: 'Stop', additionalContext: msg } }));
}

try { main(); } catch (e) { log({ error: String(e).slice(0, 200) }); }
process.exit(0);
