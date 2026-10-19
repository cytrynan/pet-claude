// test-check-stale-wording.mjs — check-stale-wording.mjs の故障注入（4象限＋新規ファイル経路＋対象外リポ）
// 使い捨て git リポを作って本体を stdin で起動する。週次ハーネス verify-hooks.mjs が自動で拾う。
// 使い方: node test-check-stale-wording.mjs [本体のパス]   → 全 PASS で exit 0
// ランナー規律: spawn 失敗・非0終了は必ず FAIL（「沈黙＝合格」の検査器を作らない）。
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync, spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const HOOK = path.resolve(process.argv[2] ?? path.join(HERE, '..', 'check-stale-wording.mjs'));
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'stale-wording-'));
const repo = path.join(tmp, 'repo');
const plain = path.join(tmp, 'plain'); // .steering の無いリポ＝対象外
const LOG = path.join(tmp, 'fires.jsonl');
const gitIn = (dir, ...a) => execFileSync('git', ['-C', dir, ...a], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] });
const g = (...a) => gitIn(repo, ...a);
const w = (rel, s, root = repo) => { fs.mkdirSync(path.dirname(path.join(root, rel)), { recursive: true }); fs.writeFileSync(path.join(root, rel), s); };
const initRepo = (dir) => { fs.mkdirSync(dir); gitIn(dir, 'init', '-q'); gitIn(dir, 'config', 'user.email', 't@t'); gitIn(dir, 'config', 'user.name', 't'); gitIn(dir, 'config', 'core.autocrlf', 'false'); };

initRepo(repo);
w('docs/functional-design.md', '# 設計\n- 2〜5 は**前に見たことのある**思い出せたカード（評価前の `intervalDays > 0`）だけで計算する。\n');
w('.steering/20261019-x/brief.md', '## 受入基準\n- A1 ②③④は「前に見たことのある」思い出せたカードだけで計算する。\n- A2 そのほか。\n');
g('add', '-A'); g('commit', '-qm', 'base');

const transcript = path.join(tmp, 't.jsonl');
const writeT = (promptAt, uses, promptText = 'やって') => fs.writeFileSync(transcript, [
  JSON.stringify({ type: 'user', timestamp: promptAt, message: { role: 'user', content: promptText } }),
  JSON.stringify({ type: 'assistant', message: { content: uses.map(u => ({ type: 'tool_use', ...u })) } }),
].join('\n') + '\n');
// reflog の HEAD@{プロンプト時刻} が直前のコミットを指すよう、プロンプト時刻の後まで待ってから変更する
const nextPrompt = () => { const at = new Date(Date.now() + 1500).toISOString(); while (Date.now() < Date.parse(at) + 1200) {} return at; };

function run(stdinObj, raw) {
  const r = spawnSync(process.execPath, [HOOK], { input: raw ?? JSON.stringify(stdinObj), encoding: 'utf8', env: { ...process.env, STALE_WORDING_LOG: LOG } });
  if (r.status !== 0 || r.error) return { spawnFail: true, err: `status=${r.status} ${String(r.error ?? r.stderr).slice(0, 200)}` };
  return { out: r.stdout };
}
const ev = (extra = {}) => ({ session_id: 's', transcript_path: transcript, cwd: repo, hook_event_name: 'Stop', stop_hook_active: false, ...extra });
const results = [];
const check = (name, r, expectFire, mustMention) => {
  const fired = !r.spawnFail && !!r.out && r.out.includes('additionalContext');
  const ok = !r.spawnFail && fired === expectFire && (!mustMention || r.out.includes(mustMention));
  results.push({ name, ok, detail: r.spawnFail ? r.err : (r.out || '(沈黙)').slice(0, 240) });
};
const logLines = () => (fs.existsSync(LOG) ? fs.readFileSync(LOG, 'utf8').trim().split('\n').filter(Boolean).map(l => JSON.parse(l)) : []);
const bashEditIn = (dir) => ({ name: 'Bash', input: { command: `cd ${dir.replace(/\\/g, '/')} && python -I - <<'EOF' ...` } });

// ① 違反: 定義を Python 風に書き換えて commit（Edit ツールを使わない）→ brief の A1 に旧文が残る → 鳴る
const p1 = nextPrompt();
w('docs/functional-design.md', '# 設計\n- 2〜5 は**前に見たことがあり、今日の評価で間隔が伸びた**思い出せたカードだけで計算する。\n');
g('commit', '-qam', 'fix def');
writeT(p1, [bashEditIn(repo)]);
check('①違反→鳴る（brief.md:2 を名指し）', run(ev()), true, '.steering/20261019-x/brief.md:2');
const l1 = logLines().at(-1);
results.push({ name: '①のログに指摘の中身（言い回し・パス・行）が残る', ok: !!l1 && l1.fired === true && l1.items?.[0]?.seg === '前に見たことのある' && l1.items[0].hits?.[0]?.line === 2, detail: JSON.stringify(l1 ?? null).slice(0, 240) });

// ③ stop_hook_active=true → 沈黙（1ターン1回まで）
check('③連鎖中→沈黙', run(ev({ stop_hook_active: true })), false);

// ④ 壊れた stdin → 沈黙・exit 0
check('④壊れた入力→沈黙', run(null, '{not json'), false);

// ② 是正: brief も新定義へ → 沈黙
w('.steering/20261019-x/brief.md', '## 受入基準\n- A1 ②③④は「前に見たことがあり、今日の評価で間隔が伸びた」思い出せたカードだけで計算する。\n- A2 そのほか。\n');
check('②是正後→沈黙', run(ev()), false);
g('commit', '-qam', 'fix brief');

// ⑤ 新規ファイル経路: ターン中に Write した調査表を、Python 風に表の閾値だけ書き換える（git には旧版が無い）
const p2 = nextPrompt();
const research = path.join(repo, 'docs/research/survey.md');
const v1 = '# 調査\n| 3 | 「定着」＝`repetition >= 3` のカード | v0.3.0 |\n\n訴求文: 「定着」は `repetition >= 3` のカードです。\n';
w('docs/research/survey.md', '# 調査\n| 3 | 「定着」＝`repetition >= 4` のカード | v0.3.0 |\n\n訴求文: 「定着」は `repetition >= 3` のカードです。\n');
writeT(p2, [{ name: 'Write', input: { file_path: research, content: v1 } }, bashEditIn(repo)], '調べて');
check('⑤新規ファイル内の閾値の取り残し→鳴る', run(ev()), true, 'repetition >= 3');

// ⑥ 対象外: .steering の無いリポで同じ違反 → 沈黙し、ログにも書かない
initRepo(plain);
w('docs/a.md', '- **前に見たことのある**カード\n', plain);
w('docs/b.md', '- 「前に見たことのある」カード\n', plain);
gitIn(plain, 'add', '-A'); gitIn(plain, 'commit', '-qm', 'base');
const p3 = nextPrompt();
w('docs/a.md', '- **前に見たことがあり**カード\n', plain);
writeT(p3, [bashEditIn(plain)]);
const before6 = logLines().length;
check('⑥ .steering の無いリポ→沈黙', run(ev({ cwd: plain })), false);
results.push({ name: '⑥のターンはログに書かない', ok: logLines().length === before6, detail: `log lines ${before6}→${logLines().length}` });

for (const r of results) console.log(`${r.ok ? 'PASS' : 'FAIL'}  ${r.name}\n      ${r.detail.replace(/\n/g, '\n      ')}`);
const ms = logLines().map(l => l.ms ?? 0);
console.log(`log lines=${ms.length} max_ms=${ms.length ? Math.max(...ms) : 0} errors=${logLines().filter(l => l.error || l.errors).length}`);
try { fs.rmSync(tmp, { recursive: true, force: true }); } catch {} // 後始末は判定の後・best-effort
process.exit(results.length > 0 && results.every(r => r.ok) ? 0 : 1);
