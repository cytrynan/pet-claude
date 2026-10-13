#!/usr/bin/env node
// check-log-companion.mjs — Stop hook: 記録規約「同じ作業内で docs/log.md に1行追記」の機械検知
//
// 契約（公式 https://code.claude.com/docs/en/hooks に準拠）:
//   stdin  = Stop イベントのJSON（transcript_path / tool_calls / stop_hook_active）
//   stdout = 違反あり かつ stop_hook_active=false → {decision:"block",reason} で終了を差し戻す（最上位キー。hookSpecificOutput 配下の decision は Stop では読まれない: 2026-10-09 判明）
//            違反あり かつ stop_hook_active=true  → {systemMessage} の警告のみ（ブロック連鎖は1回で必ず止める）
//   exit 0 固定（exit 2 は使わない。制御はJSONで行う）
//
// 検知ロジック:
//   プロジェクト内で log.md を祖先に持つ .md ファイルを編集したら、
//   その「最も近い祖先の log.md」も同セッション内で編集されていることを要求する。
//   ディレクトリのハードコードはしない（log.md の存在＝規約への参加とみなす）。
import fs from 'node:fs';
import path from 'node:path';

const ROOT = (() => {
  const r = process.env.CLAUDE_PROJECT_DIR || process.cwd();
  try { return path.resolve(r).toLowerCase(); } catch { return null; }
})();

function norm(p) {
  try { return path.resolve(String(p)).toLowerCase(); } catch { return null; }
}

function underRoot(p) {
  return ROOT != null && (p === ROOT || p.startsWith(ROOT + path.sep));
}

function editedPathOf(name, input) {
  if (!input) return null;
  if (name === 'Write' || name === 'Edit' || name === 'MultiEdit') return input.file_path ?? null;
  if (name === 'NotebookEdit') return input.notebook_path ?? null;
  return null;
}

function collectEdits(hookInput) {
  const edited = new Set();
  const add = (p) => { const n = norm(p); if (n) edited.add(n); };

  // 1) セッション全ターン: transcript(JSONL) を走査
  const tp = hookInput.transcript_path;
  if (tp && fs.existsSync(tp)) {
    for (const line of fs.readFileSync(tp, 'utf8').split('\n')) {
      if (!line.includes('"tool_use"')) continue; // 高速パス
      let obj; try { obj = JSON.parse(line); } catch { continue; }
      const content = obj?.message?.content;
      if (!Array.isArray(content)) continue;
      for (const item of content) {
        if (item?.type !== 'tool_use') continue;
        const p = editedPathOf(item.name, item.input);
        if (p) add(p);
      }
    }
  }
  // 2) 当ターン分: transcript の書き込み遅延に備えて tool_calls もマージ
  for (const tc of hookInput.tool_calls ?? []) {
    const p = editedPathOf(tc.tool_name, tc.tool_input);
    if (p) add(p);
  }
  return edited;
}

function companionLogFor(fileP) {
  let dir = path.dirname(fileP);
  while (underRoot(dir)) {
    const cand = path.join(dir, 'log.md');
    if (fs.existsSync(cand)) return norm(cand);
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return null;
}

function findViolations(edited) {
  const violations = new Map(); // logPath -> [editedFiles]
  for (const p of edited) {
    if (!underRoot(p)) continue;
    if (!p.endsWith('.md')) continue;                 // 記録＝.md のみが対象
    if (path.basename(p) === 'log.md') continue;      // log.md 自体は対象外
    if (p.split(path.sep).includes('log')) continue;  // ローテーション退避先 log/ は対象外
    const logP = companionLogFor(p);
    if (!logP || edited.has(logP)) continue;
    if (!violations.has(logP)) violations.set(logP, []);
    violations.get(logP).push(p);
  }
  return violations;
}

// 「追記したか」だけでなく「規約の形で追記したか」も見る。
// log.md の規律（`development-guidelines` スキル guides/recording.md 規律1）は
// 「書式は1形式のみ・**行頭に日付を書かない**（日付は `## YYYY-MM-DD` 見出しで表す）」。
// 追記の有無しか見ない検査は、2形式の併存＝同じ日が2か所に散る劣化を素通りさせる
// （実測: 規約が明文で禁じているのに複数セッションが末尾へ積み続け16件たまった）。
const LEADING_DATE_RE = /^\s*[*-]\s+20\d\d-\d\d-\d\d\s/;

function findFormatViolations(edited) {
  const bad = new Map(); // logPath -> [{line, text}]
  for (const p of edited) {
    if (!underRoot(p)) continue;
    if (path.basename(p) !== 'log.md') continue;
    if (p.split(path.sep).includes('log')) continue; // ローテーション退避先は対象外
    let text;
    try { text = fs.readFileSync(p, 'utf8'); } catch { continue; }
    const hits = [];
    text.split(/\r?\n/).forEach((l, i) => {
      if (LEADING_DATE_RE.test(l)) hits.push({ line: i + 1, text: l.slice(0, 60) });
    });
    if (hits.length) bad.set(p, hits);
  }
  return bad;
}

function main() {
  let hookInput;
  try { hookInput = JSON.parse(fs.readFileSync(0, 'utf8')); } catch { process.exit(0); }
  if (hookInput.hook_event_name && hookInput.hook_event_name !== 'Stop') process.exit(0);

  const edited = collectEdits(hookInput);
  const violations = findViolations(edited);
  const formatBad = findFormatViolations(edited);
  if (violations.size === 0 && formatBad.size === 0) process.exit(0);

  const lines = [];
  for (const [logP, files] of violations) {
    lines.push(`- ${logP} ← 未追記（編集済み: ${files.slice(0, 3).join(', ')}${files.length > 3 ? ` ほか${files.length - 3}件` : ''}）`);
  }
  for (const [logP, hits] of formatBad) {
    lines.push(`- ${logP} ← 行頭に日付がある行が${hits.length}件（規律: 日付は "## YYYY-MM-DD" 見出しで表し、行頭には書かない）: L${hits.slice(0, 3).map(h => h.line).join(', L')}${hits.length > 3 ? ' ほか' : ''}`);
  }
  const detail = lines.join('\n');

  if (hookInput.stop_hook_active) {
    // 既にブロック連鎖の中: 二重ブロックはせず警告で通す（無限ループの有界化）
    process.stdout.write(JSON.stringify({
      systemMessage: `⚠ log.md追記漏れが未回収のままです（check-log-companion。再ブロックはしません）:\n${detail}`,
    }));
    process.exit(0);
  }
  process.stdout.write(JSON.stringify({
    decision: 'block',
    reason: [
        '記録規約（`development-guidelines` スキル guides/recording.md）に反しています。',
        violations.size ? '【未追記】記録ファイルを編集した作業では、同じ作業内で当該ディレクトリの log.md に1行追記する。形式は「* **種別**: [ファイル](相対パス) — 要約 (アクター)」。' : '',
        formatBad.size ? '【書式】log.md の行頭に日付を書かない（日付は "## YYYY-MM-DD" 見出しで表す）。該当行の日付を削り、その日付の見出しの配下へ移してください。2形式が併存すると同じ日が2か所に散ります。' : '',
        detail,
      ].filter(Boolean).join('\n'),
  }));
  process.exit(0);
}

try { main(); } catch (e) {
  // 検知器自身の故障は隠さない（"OK"と報告する故障が最悪の形）が、終了は妨げない
  process.stdout.write(JSON.stringify({ systemMessage: `⚠ check-log-companion フック自体がエラー: ${e?.message ?? e}` }));
  process.exit(0);
}
