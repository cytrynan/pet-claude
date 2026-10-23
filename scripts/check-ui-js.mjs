// ui/*.html の中の <script>（src の無いもの）と ui/*.js を、実行せずに構文だけ検査する（品質ゲートの 1 段）。
//
// なぜ: 2026-10-22 に chat.html の文字列へ本物の改行が入り（Bash の heredoc から書いた "\n" が潰れた）、
// チャット欄のスクリプト全体が SyntaxError で動かなくなった。Rust のテストと故障注入は画面のスクリプトを
// 通らない（送信は Rust 側から呼ぶ）ので、どれも気づかなかった（Codex の検証が node で見つけた）。
// 使い方: node scripts/check-ui-js.mjs [ui のフォルダ]   … 壊れた所を出して exit 1
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import vm from "node:vm";

const dir = process.argv[2] ?? join(import.meta.dirname, "..", "ui");
let bad = 0;
let checked = 0;
for (const name of readdirSync(dir).sort()) {
  const path = join(dir, name);
  const blocks = [];
  if (name.endsWith(".js")) {
    blocks.push({ code: readFileSync(path, "utf8"), line: 1 });
  } else if (name.endsWith(".html")) {
    const html = readFileSync(path, "utf8");
    // src 属性の無い <script> だけ（src 付きは別ファイルとして見る）。属性は名前で見る
    // （/\bsrc=/ だと data-src= や title="src=x" も外部と見なし、壊れたインラインを見逃した。2026-10-23 検証係の指摘）
    for (const m of html.matchAll(/<script(\s[^>]*)?>([\s\S]*?)<\/script>/g)) {
      if (m[1] && hasSrcAttribute(m[1])) continue;
      const line = html.slice(0, m.index).split("\n").length;
      blocks.push({ code: m[2], line });
    }
  }
  for (const b of blocks) {
    checked++;
    try {
      // 実行はしない（Script を作るだけ＝構文の解析だけ）
      new vm.Script(b.code, { filename: path, lineOffset: b.line - 1 });
    } catch (e) {
      bad++;
      console.error(`${path}: ${e.message}`);
      const at = String(e.stack).split("\n")[0];
      console.error(`  ${at}`);
    }
  }
}
/** 開きタグの属性の並びに、名前がちょうど src の属性があるか（値の中の文字は見ない）。 */
function hasSrcAttribute(attrs) {
  for (const a of attrs.matchAll(/([^\s"'=<>\/]+)(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s"'=<>`]+))?/g)) {
    if (a[1].toLowerCase() === "src") return true;
  }
  return false;
}

if (checked === 0) {
  console.error(`検査する script が 1 つも無い: ${dir}`);
  process.exit(1);
}
console.log(`ui scripts: ${checked} checked, ${bad} broken`);
process.exit(bad ? 1 : 0);
