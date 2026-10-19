// stale-wording-core.mjs — 「消えた言い回しが記録文書に残っていないか」の判定（純関数。check-stale-wording.mjs が使う）
//
// 入力: 変更前後のファイル内容の組 [{path, before, after}] と、検索対象の文書 [{path, text, before?}]
// 出力: [{segment, from, hits:[{path, line, text}]}]
//
// 考え方: あるファイルから消えた「言い回し」(segment) が、検索対象の文書の「このターンで触っていない行」に
//         まだ残っていたら報告する（定義・閾値を変えたのに受入基準・設計書が旧いまま、の型）。
// 規則の根拠（どれも故障注入か過去コミットの再生で見つけたもの）は各行のコメント。

// 区切り: 句読点・括弧・引用符・Markdown 記号・改行。空白と比較演算子（> < =）は区切らない
// （`repetition >= 3` を1語に保つ。> と = で割ると比較式を一度も拾えない＝故障注入で発見）。行頭の引用記号 > は segmentsOf で剥がす
const SPLIT_RE = /[\n、。，．,;；:：「」『』（）()\[\]【】{}|*`#"“”'‘’＝→←・]+/u;
const MIN_LEN = 6; // 文字数（コードポイント）

function removedLines(before, after) {
  const cnt = new Map();
  for (const l of after.split('\n')) cnt.set(l, (cnt.get(l) ?? 0) + 1);
  const out = [];
  for (const l of before.split('\n')) {
    const c = cnt.get(l) ?? 0;
    if (c > 0) cnt.set(l, c - 1); else out.push(l);
  }
  return out;
}

export function segmentsOf(line) {
  const segs = [];
  for (let s of line.split(SPLIT_RE)) {
    s = s.trim().replace(/^[->+\s\d.)]+(?=\D)/u, '').trim(); // 箇条書き記号・引用記号・番号を外す
    if ([...s].length < MIN_LEN) continue;
    if (!/[\p{L}]/u.test(s)) continue; // 数字・記号だけは捨てる
    if (/^https?\S*$/.test(s) || /^[\w./\\-]+\.(png|md|ts|mjs|json|html)$/.test(s)) continue; // URL・ファイル名
    const cjk = /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}]/u.test(s);
    // 英字だけの断片は3語以上（1〜2語は一般語で鳴る）。比較式（intervalDays > 0 等）は語数によらず残す
    if (!cjk && s.split(/\s+/).length < 3 && !/[<>]=?|===?|!==?/.test(s)) continue;
    if (/^v?\d+(\.\d+)+\S*$/.test(s) || /[0-9A-Fa-f]{16,}/.test(s) || /[\\/]/.test(s)) continue; // 版番号・ハッシュ・パス
    segs.push(s);
  }
  return segs;
}

const countOf = (s, seg) => s.split(seg).length - 1;
const lf = (s) => (s == null ? s : s.replace(/\r\n?/g, '\n')); // autocrlf: 作業ツリーは CRLF・git show は LF になりうる

// changes: [{path, before, after}]（before=null は新規・after=null は削除＝どちらも言い回しの出どころにしない）
// targets: [{path, text, before?}]（before があれば、その中に既にあった行＝このターンで触っていない行だけを見る）
// （撤去した案: 「新旧が同じ行に並ぶ行＝対照表として除外」。偶然の言い回しの一致で本物の取り残しを消した）
export function staleSegments(changesIn, targetsIn) {
  const changes = changesIn.map(c => ({ ...c, before: lf(c.before), after: lf(c.after) }));
  const targets = targetsIn.map(t => ({ ...t, text: lf(t.text), before: lf(t.before) }));
  const found = new Map();
  for (const c of changes) {
    if (c.before == null || c.after == null) continue;
    if (isStatusDoc(c.path)) continue; // 状況メモ・履歴は毎回書き換えるので出どころにしない
    for (const l of removedLines(c.before, c.after)) {
      for (const seg of segmentsOf(l)) {
        // そのファイルでの出現回数が減っていない＝移動・並べ替えだけ
        const nAfter = countOf(c.after, seg);
        if (nAfter >= countOf(c.before, seg)) continue;
        // 同じファイル内の取り残しを候補にするのは、数値・比較式を含む断片（閾値）だけ。文章の断片は
        // そのファイルから完全に消えた時だけ（重複段落の削除で数百件鳴った＝再生で発見）
        if (nAfter > 0 && !/\d|[<>]=?|[≥≤]/.test(seg)) continue;
        if (!found.has(seg)) found.set(seg, { segment: seg, from: c.path, hits: [] });
      }
    }
  }
  const results = [];
  for (const r of found.values()) {
    for (const t of targets) {
      const old = t.before == null ? null : new Set(t.before.split('\n'));
      t.text.split('\n').forEach((ln, i) => {
        if (!ln.includes(r.segment)) return;
        if (old && !old.has(ln)) return; // このターンで書いた行＝移動先・新しい記述なので古い記述ではない
        r.hits.push({ path: t.path, line: i + 1, text: ln.trim().slice(0, 160) });
      });
    }
    if (r.hits.length) results.push(r);
  }
  return results;
}

// 履歴の記録（log.md・*-log.md・CHANGELOG・PROJECT_LOG・日付入りのファイル＝その日の写し）
export function isHistoryDoc(p) {
  const b = p.replace(/\\/g, '/').split('/').pop().toLowerCase();
  return b === 'log.md' || b.endsWith('-log.md') || b.startsWith('changelog') || b === 'project_log.md'
    || /\d{8}|\d{4}-\d{2}-\d{2}/.test(b);
}

// 状況メモ（再開メモ・進捗・プロジェクトログ）。出どころにも検索先にもしない
export function isStatusDoc(p) {
  const b = p.replace(/\\/g, '/').split('/').pop().toLowerCase();
  return isHistoryDoc(p) || /^(resume|progress|project_log|todo|handover)[\w-]*\.md$/.test(b);
}

// .steering/ 直下の作業フォルダ名のうち名前順で最大（YYYYMMDD-<task> なので＝いま進めている作業）
export function newestSteeringDir(relPaths) {
  let best = null;
  for (const r of relPaths) { const m = r.replace(/\\/g, '/').match(/^\.steering\/([^/]+)\//); if (m && (!best || m[1] > best)) best = m[1]; }
  return best;
}

// 検索対象: このターンで変更した .md と、変更があった／最新の .steering/<作業>/ の .md だけ。
// 触っていない文書の全面走査はしない（docs/ 全体に広げると発火が約4倍・ほぼ雑音になった＝再生で実測）
export function isNarrowTarget(rel, touchedSteeringDirs, changedMap) {
  const p = rel.replace(/\\/g, '/');
  if (!p.endsWith('.md') || isStatusDoc(p)) return false;
  if (changedMap.has(rel)) return true;
  const m = p.match(/^\.steering\/([^/]+)\//);
  return !!(m && touchedSteeringDirs.has(m[1]));
}
