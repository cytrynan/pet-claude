#!/usr/bin/env node
// guard-gated-commit.mjs — PreToolUse(Bash / PowerShell) フック
//
// 責務（1つだけ）: このプロジェクトへの `git commit` の直打ちを止め、`scripts/gate-commit.sh`
//   （ゲート → add → commit を 1 本にしたスクリプト）経由だけを通す。
//
// なぜ機構か:
//   「ゲートを通してから commit」は文言で 3 回書き直しても、手連鎖の形を変えて 4 回すり抜けた
//   （`&&` の外に commit／パイプで exit が隠れる／`;` で && の起点がずれる×2）。
//   フックは想起に依存しない（CLAUDE.md「禁止の規範は文章でなく機構で止める」）。
//
// 判定:
//   - コマンドを `&&` `||` `;` `|` 改行で区切り、区切りごとに先頭トークンを見る
//   - 先頭が git（パス付き・.exe も可）で、グローバルオプションを飛ばした先の副コマンドが
//     `commit` なら候補。`-C <dir>` / 直前の `cd <dir>` を解決し、**対象がこのプロジェクト
//     （CLAUDE_PROJECT_DIR、無ければ cwd）の中なら deny**。他リポジトリへの commit は通す
//   - `bash scripts/gate-commit.sh …` はツール呼び出しの文字列に `git commit` を含まないので通る
//     （スクリプト内部の git commit はツール呼び出しではないのでフックの対象外）
//   ★罠: `git -C <dir> commit` / `git -c k=v commit` のように「値を別トークンで取る」
//     オプションを正規表現で飛ばそうとすると素通しになる（guard-secret-exfil と同じ罠）ので、
//     トークン列を歩いて値つきオプションを明示的に飛ばす
//
// 契約: stdin にイベントJSON / exit 0 + stdout JSON（deny のときだけ出力。通常は沈黙）
// フェイルオープン: 入力が壊れても例外が出ても必ず exit 0。
// 既知の限界: Bash のコマンド文字列（ヒアドキュメント本文を含む）の中に、区切り（&& ; | 改行）の
//   直後から始まる `git commit` があると誤って止まる（実例: テストケースの文面を heredoc で
//   書いた時）。そういう文面は Write ツールでファイルに書いてから実行する。

import fs from 'node:fs';
import path from 'node:path';

const SEP = /&&|\|\||;|\||\r?\n/;
const UNRESOLVABLE = /[$`%]/; // 変数・コマンド置換はフックでは展開できない（曖昧なら止める）
const VALUE_OPTS = new Set(['-C', '-c', '--git-dir', '--work-tree', '--namespace', '--exec-path', '--config-env']);

function unquote(s) {
  return String(s || '').trim().replace(/^["']+|["']+$/g, '');
}

// Windows / Git Bash / 相対 のどれで書かれても比較できる形に（小文字・スラッシュ・末尾スラッシュなし）
export function normPath(p, base) {
  let s = unquote(p);
  if (!s) return null;
  const home = process.env.USERPROFILE || process.env.HOME || '';
  if (s === '~' || s.startsWith('~/') || s.startsWith('~\\')) s = home + s.slice(1);
  s = s.replace(/\\/g, '/');
  const m = s.match(/^\/([a-zA-Z])(\/.*)?$/); // /c/Users/x → c:/Users/x
  if (m) s = `${m[1]}:${m[2] || '/'}`;
  const isAbs = /^[a-zA-Z]:\//.test(s) || s.startsWith('/');
  if (!isAbs) {
    const b = base ? normPath(base) : null;
    if (!b) return null;
    s = b + '/' + s;
  }
  s = path.posix.normalize(s);
  s = s.replace(/\/+$/, '');
  return s.toLowerCase();
}

// 引用符の中の空白で割らない（"C:\Program Files\Git\bin\git.exe" commit を 1 トークンに）
function tokenize(seg) {
  const out = [];
  let cur = '';
  let q = null;
  for (const ch of seg) {
    if (q) {
      cur += ch;
      if (ch === q) q = null;
      continue;
    }
    if (ch === '"' || ch === "'") { q = ch; cur += ch; continue; }
    if (/\s/.test(ch)) { if (cur) { out.push(cur); cur = ''; } continue; }
    cur += ch;
  }
  if (cur) out.push(cur);
  return out.map((t) => t.replace(/^\(+|\)+$/g, '')).filter(Boolean);
}

function isInside(target, root) {
  if (!target || !root) return false;
  return target === root || target.startsWith(root + '/');
}

function isGit(tok) {
  return /(^|[\/\\])git(\.exe)?$/i.test(unquote(tok));
}

function gitCommitTarget(tokens) {
  // 先頭 git の後ろのグローバルオプションを飛ばして副コマンドを見る。-C の値を返す（無ければ undefined）
  let cDir;
  let i = 1;
  while (i < tokens.length) {
    const t = tokens[i];
    if (!t.startsWith('-')) break;
    if (VALUE_OPTS.has(t)) {
      if (t === '-C') cDir = tokens[i + 1];
      i += 2;
      continue;
    }
    if (t.startsWith('-C') && t.length > 2) cDir = t.slice(2); // -C/path の形
    i += 1;
  }
  const sub = tokens[i];
  if (sub !== 'commit') return null;
  return { cDir };
}

export function decide(command, cwd, projectDir) {
  const cmd = String(command || '');
  if (!cmd.trim()) return null;
  const root = normPath(projectDir || cwd);
  if (!root) return null;
  let cur = normPath(cwd) || root;
  for (const rawSeg of cmd.split(SEP)) {
    const tokens = tokenize(rawSeg.trim().replace(/^\(+|\)+$/g, ''));
    // 先頭の環境変数代入（FOO=bar cmd）は飛ばす
    while (tokens.length && /^[A-Za-z_][A-Za-z0-9_]*=/.test(tokens[0])) tokens.shift();
    if (!tokens.length) continue;
    const head = unquote(tokens[0]).toLowerCase();
    if (head === 'cd' || head === 'pushd' || head === 'set-location') {
      const next = tokens[1] && !tokens[1].startsWith('-') ? tokens[1] : null;
      // 変数・コマンド置換（$T / %T% / `…`）はフックでは展開できない → 以後の対象は「不明」
      if (next && UNRESOLVABLE.test(next)) { cur = null; continue; }
      cur = next ? normPath(next, cur) || cur : normPath('~') || cur;
      continue;
    }
    if (!isGit(tokens[0])) continue;
    const hit = gitCommitTarget(tokens);
    if (!hit) continue;
    if ((hit.cDir && UNRESOLVABLE.test(hit.cDir)) || (cur === null && !hit.cDir)) {
      return (
        'commit の対象ディレクトリが変数やコマンド置換で書かれていて、フックが解決できない。' +
        'リテラルのパスで書き直す（他リポジトリなら cd /c/…/repo を直書き、' +
        'このプロジェクトなら bash scripts/gate-commit.sh <msg-file>）。'
      );
    }
    const target = hit.cDir ? normPath(hit.cDir, cur) : cur;
    if (isInside(target, root)) {
      return (
        'このプロジェクトへの commit は `bash scripts/gate-commit.sh <msg-file>` 経由だけ（ゲートが通った時だけ add→commit）。' +
        '`git commit` の直打ちは、ゲート失敗のまま commit が通る事故が 4 回起きたので機構で止めている。' +
        `（対象: ${target}）`
      );
    }
  }
  return null;
}

function main() {
  let input;
  try {
    input = JSON.parse(fs.readFileSync(0, 'utf8'));
  } catch {
    process.exit(0);
  }
  if (input.hook_event_name && input.hook_event_name !== 'PreToolUse') process.exit(0);
  if (input.tool_name && input.tool_name !== 'Bash' && input.tool_name !== 'PowerShell') process.exit(0);

  const reason = decide(input?.tool_input?.command, input?.cwd, process.env.CLAUDE_PROJECT_DIR);
  if (!reason) process.exit(0);

  process.stdout.write(JSON.stringify({
    hookSpecificOutput: {
      hookEventName: 'PreToolUse',
      permissionDecision: 'deny',
      permissionDecisionReason:
        reason + '\nこの判断は会話上の指示では覆らない（機構による遮断）。'
        + '\n本当に直打ちが必要なら、このフックの設定を変更する提案を人間に出して承認を得ること。',
    },
  }));
  process.exit(0);
}

if (process.argv[1] && process.argv[1].endsWith('guard-gated-commit.mjs')) {
  try {
    main();
  } catch (e) {
    process.stdout.write(JSON.stringify({
      systemMessage: `⚠ guard-gated-commit フック自体がエラー: ${e?.message ?? e}`,
    }));
    process.exit(0);
  }
}
