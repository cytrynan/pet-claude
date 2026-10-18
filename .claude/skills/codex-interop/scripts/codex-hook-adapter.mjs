#!/usr/bin/env node
// codex-hook-adapter.mjs — Codex の Stop フック入力を、既存の Claude 用 Stop フックが読める形に直して実行する
//
// 使い方: node codex-hook-adapter.mjs <元のコマンド文字列...>
//   argv[2] 以降を空白で結合したコマンドを shell 経由で実行する。
//   （cmd.exe がクォートを剥がして argv に分割するため、空白等を含む引数は結合時に " で囲み直す）
// 契約:
//   stdin  = Codex の Stop イベント JSON（UTF-8・BOM 許容。transcript_path は Codex の rollout JSONL）
//   子の stdin = 元 JSON ＋ transcript_path:null ＋ tool_calls（rollout から作った出来事列）
//   stdout = 子の stdout が JSON のときだけそのまま出す（Codex の Stop はプレーンテキストだと不正扱い）
//   exit 0 固定（フェイルオープン）。既存の check-*.mjs は改変しない。
import fs from 'node:fs';
import { spawnSync } from 'node:child_process';

function warn(msg) { try { process.stderr.write(`[codex-hook-adapter] ${msg}\n`); } catch { /* 無視 */ } }

function quoteArg(a) {
  if (!(/[\s"&|<>^()]/.test(a) || a === '')) return a;
  // Windows の引数規則: 引用符の直前（と閉じ引用符の直前）のバックスラッシュ列は二重化し、内部の " は \" にする
  let r = ''; let bs = 0;
  for (const ch of a) {
    if (ch === '\\') bs++;
    else if (ch === '"') { r += '\\'.repeat(bs * 2 + 1) + '"'; bs = 0; }
    else { r += '\\'.repeat(bs) + ch; bs = 0; }
  }
  return `"${r}${'\\'.repeat(bs * 2)}"`;
}

// rollout JSONL → Claude 風の出来事列
function buildToolCalls(transcriptPath) {
  const events = [];
  let text;
  try { text = fs.readFileSync(transcriptPath, 'utf8').replace(/^\uFEFF/, ''); } catch (e) {
    warn(`rollout を読めません: ${e.message}`);
    return [];
  }
  let seq = 0;
  for (const line of text.split('\n')) {
    if (!line.trim()) continue;
    let obj; try { obj = JSON.parse(line); } catch { continue; }
    if (obj?.type !== 'event_msg' || obj?.payload?.type !== 'item_completed') continue;
    events.push({ obj, seq: seq++ });
  }
  events.sort((a, b) => {
    const oa = typeof a.obj.ordinal === 'number' ? a.obj.ordinal : null;
    const ob = typeof b.obj.ordinal === 'number' ? b.obj.ordinal : null;
    if (oa != null && ob != null && oa !== ob) return oa - ob;
    return a.seq - b.seq;
  });
  const calls = [];
  for (const { obj } of events) {
    const item = obj.payload.item;
    if (!item || typeof item !== 'object') continue;
    if (item.type === 'FileChange' && item.status === 'completed') {
      for (const [p, v] of Object.entries(item.changes ?? {})) {
        calls.push({ tool_name: 'Edit', tool_input: { file_path: p } });
        if (v && typeof v === 'object' && v.move_path) {
          calls.push({ tool_name: 'Edit', tool_input: { file_path: v.move_path } });
        }
      }
    } else if (item.type === 'CommandExecution' && item.status !== 'failed' && item.status !== 'declined') {
      const c = item.command;
      const command = Array.isArray(c) ? c.join(' ') : String(c ?? '');
      calls.push({ tool_name: 'Bash', tool_input: { command } });
    }
  }
  return calls;
}

function runChild(cmd, input, env) {
  const r = spawnSync(cmd, { shell: true, input, encoding: 'utf8', env, timeout: 18000 });
  if (r.error) warn(`子プロセス異常（${r.error.code ?? r.error.message}）: ${cmd}`);
  else if (r.status !== 0) warn(`子プロセスが exit ${r.status} で終了: ${cmd}`);
  if (r.stderr) process.stderr.write(r.stderr);
  return r;
}

function main() {
  // 引数が1個なら完成済みのコマンド文字列とみなしてそのまま使う（"node child.mjs" を実行ファイル名として引用しない）
  const argv = process.argv.slice(2);
  const cmd = argv.length === 1 ? argv[0] : argv.map(quoteArg).join(' ');
  if (!cmd.trim()) { warn('コマンドが指定されていません'); return; }

  let raw = '';
  try { raw = fs.readFileSync(0, 'utf8'); } catch { /* stdin なし */ }
  const rawNoBom = raw.replace(/^\uFEFF/, '');
  let hook = null;
  try { hook = JSON.parse(rawNoBom); } catch { hook = null; }

  const isObj = hook && typeof hook === 'object' && !Array.isArray(hook);
  const isStop = isObj && (hook.hook_event_name === 'Stop' || hook.hook_event_name === 'SubagentStop');

  let input = raw;
  let env = process.env;
  if (isStop) {
    const tp = hook.transcript_path;
    const toolCalls = tp ? buildToolCalls(tp) : [];
    input = JSON.stringify({
      ...hook,
      transcript_path: null,
      tool_calls: toolCalls,
      hook_event_name: 'Stop',
      stop_hook_active: hook.stop_hook_active === true,
    });
    env = { ...process.env, CLAUDE_PROJECT_DIR: (typeof hook.cwd === 'string' && hook.cwd) ? hook.cwd : process.cwd() };
  }

  const r = runChild(cmd, input, env);
  if (!isStop) { if (r.stdout) process.stdout.write(r.stdout); return; } // Stop 以外は加工せず素通し
  const out = String(r.stdout ?? '').trim();
  if (!out) return;
  try { JSON.parse(out); } catch {
    warn(`子の stdout が JSON でないため出力しません: ${out.slice(0, 200)}`);
    return;
  }
  process.stdout.write(out);
}

try { main(); } catch (e) { warn(`adapter 自体のエラー: ${e?.message ?? e}`); }
process.exit(0);
