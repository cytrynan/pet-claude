#!/usr/bin/env node
// test-guard-gated-commit.mjs — 故障注入テスト（4象限）
//   ①違反 → deny  ②正常 → 沈黙（偽陽性の番人）  ③設定（プロジェクト根・ツール名） ④壊れた入力 → 無害
// 使い方: node .claude/scripts/tests/test-guard-gated-commit.mjs   （終了コード 0=PASS / 1=FAIL）

import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const HOOK = path.join(HERE, '..', 'guard-gated-commit.mjs');
const ROOT = 'C:/proj/app'; // 架空のプロジェクト根（実在しなくてよい＝文字列比較だけ）

function run(payload, env = {}) {
  const r = spawnSync(process.execPath, [HOOK], {
    input: typeof payload === 'string' ? payload : JSON.stringify(payload),
    encoding: 'utf8',
    env: { ...process.env, CLAUDE_PROJECT_DIR: ROOT, ...env },
  });
  return { code: r.status, out: (r.stdout || '').trim() };
}

const ev = (command, extra = {}) => ({
  hook_event_name: 'PreToolUse',
  tool_name: 'Bash',
  tool_input: { command },
  cwd: ROOT,
  ...extra,
});

function isDeny(out) {
  if (!out) return false;
  try {
    return JSON.parse(out)?.hookSpecificOutput?.permissionDecision === 'deny';
  } catch {
    return false;
  }
}

const cases = [
  // ① 違反 → deny
  ['DENY 直打ち', ev('git commit -m "x"'), true, {}],
  ['DENY && 連鎖の末尾（事故の形 1）', ev('git add -A && git commit -q -F msg.txt'), true, {}],
  ['DENY ; で起点がずれた形（事故の形 2）', ev('bash scripts/gate.sh > log; echo "exit=$?"; grep GATE log && git add -A && git commit -F m'), true, {}],
  ['DENY git -C . commit（★値つきオプションの罠）', ev('git -C . commit -m x'), true, {}],
  ['DENY git -C <絶対パス=プロジェクト> commit', ev(`git -C ${ROOT} commit -m x`), true, {}],
  ['DENY git -c k=v commit（-c も値を取る）', ev('git -c user.name=x commit -m x'), true, {}],
  ['DENY cd プロジェクト && commit（Git Bash 形式のパス）', ev('cd /c/proj/app && git commit -m x', { cwd: 'C:/elsewhere' }), true, {}],
  ['DENY cd サブディレクトリ && commit', ev('cd src && git commit -m x'), true, {}],
  ['DENY --amend', ev('git commit --amend --no-edit'), true, {}],
  ['DENY パス付き git.exe', ev('"C:\\Program Files\\Git\\bin\\git.exe" commit -m x'), true, {}],
  ['DENY PowerShell ツール経由', ev('git commit -m x', { tool_name: 'PowerShell' }), true, {}],
  ['DENY 環境変数代入つき', ev('GIT_AUTHOR_NAME=x git commit -m x'), true, {}],
  ['DENY cd が変数（解決不能は止める・偽陽性の実例 2026-09-30）', ev('cd "$T" && git commit -m x'), true, {}],
  ['DENY -C が変数', ev('git -C "$REPO" commit -m x'), true, {}],
  ['DENY cd がコマンド置換', ev('cd "$(pwd)" && git commit -m x'), true, {}],

  // ② 正常系 → 沈黙
  ['PASS gate-commit.sh 経由', ev('bash scripts/gate-commit.sh msg.txt'), false, {}],
  ['PASS gate-commit.sh 経由（絶対パスのメッセージ）', ev('bash scripts/gate-commit.sh "C:/tmp/m.txt" > log 2>&1; echo $?'), false, {}],
  ['PASS git status', ev('git status --short'), false, {}],
  ['PASS git log', ev('git log --oneline -1'), false, {}],
  ['PASS 他リポジトリへ -C', ev('git -C C:/other/repo commit -m x'), false, {}],
  ['PASS 他リポジトリへ cd', ev('cd /c/other/repo && git commit -q -m x'), false, {}],
  ['PASS 他リポジトリへ cd（Windows パス）', ev('cd C:\\Users\\meer0\\.claude && git commit -m x'), false, {}],
  ['PASS 他リポジトリへ cd の後に export を挟む', ev('cd /c/other/repo && export X=1 && git commit -m x'), false, {}],
  ['PASS echo の中の文字列', ev('echo "git commit"'), false, {}],
  ['PASS ヒアドキュメント本文（行頭が - ）', ev("cat > m.txt <<'EOF'\n- git commit を使わない\nEOF"), false, {}],
  ['PASS git push（commit ではない）', ev('git push origin main'), false, {}],
  ['PASS commit の語を含む別コマンド', ev('gh pr view --json commits'), false, {}],

  // ③ 設定
  ['PASS プロジェクト根が別（CLAUDE_PROJECT_DIR）なら cwd での commit も通す', ev('git commit -m x', { cwd: 'C:/other' }), false, {}],
  ['DENY CLAUDE_PROJECT_DIR 未設定なら cwd を根にする', ev('git commit -m x'), true, { CLAUDE_PROJECT_DIR: '' }],

  // ④ 頑健性
  ['ROBUST 壊れた stdin', 'not-json{{', false, {}],
  ['ROBUST 空 stdin', '', false, {}],
  ['ROBUST 別ツール', { hook_event_name: 'PreToolUse', tool_name: 'Write', tool_input: { file_path: 'x' } }, false, {}],
  ['ROBUST 別イベント', { hook_event_name: 'Stop' }, false, {}],
  ['ROBUST command 欠落', { hook_event_name: 'PreToolUse', tool_name: 'Bash', tool_input: {} }, false, {}],
];

let pass = 0;
let fail = 0;
for (const [name, payload, wantDeny, env] of cases) {
  const r = run(payload, env);
  const got = isDeny(r.out);
  const ok = got === wantDeny && r.code === 0;
  if (ok) { pass++; console.log('  PASS  ' + name); }
  else { fail++; console.log(`  FAIL  ${name}  (want deny=${wantDeny} got=${got} exit=${r.code} out=${r.out.slice(0, 120)})`); }
}

console.log(`\nguard-gated-commit: ${pass} passed, ${fail} failed`);
process.exit(fail === 0 ? 0 : 1);
