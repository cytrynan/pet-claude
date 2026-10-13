#!/usr/bin/env node
// hook-fault-injection.mjs — check-log-companion.mjs（同ディレクトリ）の故障注入テスト
// フックを変更したら必ず再実行する。週次ハーネスに合流させてもよい。
// 象限: 違反→block / 是正→沈黙 / stop_hook_active→警告 / 壊れたstdin→無害 / log.md不在→不活性
//       ＋実運用経路2本（tool_callsマージ＝transcript書き込み遅延時の唯一の検知経路／祖先ディレクトリ遡り）。
// ランナー規律: spawn失敗・非0終了は必ずFAIL（「沈黙=合格」の検査器を作らない）。
// 検査器自体の検証（零点アーム）: 対象パスを壊して全ケースFAILになることを一度実測してから信用する。
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const HOOK = path.join(here, 'check-log-companion.mjs');

const FIX = fs.mkdtempSync(path.join(os.tmpdir(), 'hook-fixture-'));
const BUNDLE = path.join(FIX, 'docs');          // log.md あり＝規約に参加
const NESTED = path.join(BUNDLE, 'sub');        // 祖先の log.md を継承する下位ディレクトリ
const FREE = path.join(FIX, 'free');            // log.md なし＝不活性であるべき
fs.mkdirSync(NESTED, { recursive: true });
fs.mkdirSync(FREE, { recursive: true });
fs.writeFileSync(path.join(BUNDLE, 'log.md'), '# log\n');
fs.writeFileSync(path.join(BUNDLE, 'note.md'), 'note\n');
fs.writeFileSync(path.join(NESTED, 'deep.md'), 'deep\n');
fs.writeFileSync(path.join(FREE, 'memo.md'), 'memo\n');

const toolUse = (p) => JSON.stringify({ message: { content: [{ type: 'tool_use', name: 'Write', input: { file_path: p } }] } });
const tViolation = path.join(FIX, 't_violation.jsonl');
const tFixed = path.join(FIX, 't_fixed.jsonl');
const tFree = path.join(FIX, 't_free.jsonl');
fs.writeFileSync(tViolation, toolUse(path.join(BUNDLE, 'note.md')) + '\n');
fs.writeFileSync(tFixed, toolUse(path.join(BUNDLE, 'note.md')) + '\n' + toolUse(path.join(BUNDLE, 'log.md')) + '\n');
fs.writeFileSync(tFree, toolUse(path.join(FREE, 'memo.md')) + '\n');

const input = (transcript, chained, toolCalls) => JSON.stringify({
  hook_event_name: 'Stop', transcript_path: transcript ?? path.join(FIX, 'no-such-transcript.jsonl'),
  tool_calls: toolCalls ?? [], stop_hook_active: !!chained,
});

let pass = 0, fail = 0;
function run(name, stdin, check) {
  const r = spawnSync('node', [HOOK], { input: stdin, env: { ...process.env, CLAUDE_PROJECT_DIR: FIX }, encoding: 'utf8' });
  if (r.error) { fail++; console.log(`FAIL ${name}: spawn error ${r.error.message}`); return; }
  if (r.status !== 0) { fail++; console.log(`FAIL ${name}: exit ${r.status} stderr=${r.stderr}`); return; }
  const problem = check(r.stdout);
  if (problem) { fail++; console.log(`FAIL ${name}: ${problem}\n  stdout=${JSON.stringify(r.stdout)}`); }
  else { pass++; console.log(`PASS ${name}`); }
}
const parse = (s) => { try { return JSON.parse(s); } catch { return null; } };
const expectBlock = (out) => {
  const j = parse(out);
  if (!j) return 'stdoutがJSONでない';
  if (j.decision !== 'block') return 'decision:block が出ていない（最上位キー）';
  return null;
};

run('1 違反→block', input(tViolation, false), expectBlock);
run('2 是正→沈黙', input(tFixed, false), (out) => out.trim() === '' ? null : '是正済みなのに出力がある');
run('3 stop_hook_active→警告のみ', input(tViolation, true), (out) => {
  const j = parse(out);
  if (!j) return 'stdoutがJSONでない';
  if (j.decision) return '連鎖中に再blockしている（有界化が壊れている）';
  if (!j.systemMessage) return 'systemMessage 警告が無い';
  return null;
});
run('4 壊れたstdin→無害', 'this is not json {', (out) => out.trim() === '' ? null : '壊れた入力で出力を出している');
run('5 log.mdの無い場所→不活性', input(tFree, false), (out) => out.trim() === '' ? null : 'log.md不在ディレクトリでブロック/警告を出している');
run('6 tool_callsのみ（transcript遅延経路）→block',
  input(null, false, [{ tool_name: 'Write', tool_input: { file_path: path.join(BUNDLE, 'note.md') } }]), expectBlock);
run('7 下位ディレクトリ→祖先log.mdへ遡ってblock',
  input(null, false, [{ tool_name: 'Write', tool_input: { file_path: path.join(NESTED, 'deep.md') } }]), expectBlock);

fs.rmSync(FIX, { recursive: true, force: true });
console.log(`\n結果: PASS=${pass} FAIL=${fail}`);
process.exit(fail === 0 ? 0 : 1);
