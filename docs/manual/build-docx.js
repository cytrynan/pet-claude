// マニュアルの初版（pet-claude-manual.docx）を docx（npm）で組む。
// 使い方: node docs/manual/build-docx.js [出力パス]  → 既定は docs/manual/pet-claude-manual.docx
//
// 正本は docx の方（K が Word で直す）。このスクリプトは最初の 1 回と、構成を丸ごと作り直す時にだけ使う。
// 走らせると K の手直しを上書きするので、普段は走らせない（docs/manual/README.md）。
// PDF は export-pdf.ps1 で docx から書き出す。図（images/diagrams/*.png）は render-diagrams.js が diagrams.html から作る。
//
// 見た目は apple-design（HIG）: 文字の大きさの差で階層を作り、罫線は髪の毛の太さ、色は意味のある所（状態のチップ）だけ。
// 字体は Yu Gothic UI（Noto Sans JP は可変フォントで Word が解決できず Calibri に落ちる・2026-10-28 実測）。
// docx は npm のグローバル（%APPDATA%\npm\node_modules\docx）を使う（scripts/verify/render-ui.js と同じ流儀）。
const fs = require("fs");
const path = require("path");
const D = require(path.join(process.env.APPDATA, "npm/node_modules/docx"));
const {
  AlignmentType, BorderStyle, Bookmark, Document, Footer, HeadingLevel, ImageRun, InternalHyperlink,
  LevelFormat, PageNumber, Packer, Paragraph, ShadingType, Table, TableCell, TableRow, TextRun,
  VerticalAlign, WidthType,
} = D;

const OUT = path.resolve(process.argv[2] || path.join(__dirname, "pet-claude-manual.docx"));
const IMG = path.join(__dirname, "images");

// ---- 寸法 ---------------------------------------------------------------
// Word の内部単位は DXA（1 インチ = 1440）。画像は 96dpi のピクセル。
const mm = (v) => Math.round((v / 25.4) * 1440);
const mmPx = (v) => Math.round((v / 25.4) * 96);
const PAGE_W = mm(210), PAGE_H = mm(297);
const MARGIN = { top: mm(20), right: mm(18), bottom: mm(20), left: mm(18) };
const CONTENT_W = PAGE_W - MARGIN.left - MARGIN.right; // 174mm
const CONTENT_MM = 174;

// ---- 字体と色（apple-design の値） -----------------------------------------
const FONT = "Yu Gothic UI";
const MONO = { ascii: "Cascadia Mono", hAnsi: "Cascadia Mono", cs: "Cascadia Mono", eastAsia: "Yu Gothic UI" };
const LABEL = "1D1D1F", LABEL2 = "6E6E73", SEP = "D2D2D7", BG2 = "F5F5F7", TINT = "0071E3";
// 状態のチップ。HIG の system tint を、紙の上で読める濃さ（文字）と薄さ（地）にした。色だけに頼らず必ず言葉を添える
const CHIP = {
  blue: ["0071E3", "E5F1FF"], green: ["1F8F45", "E3F7EA"], orange: ["C25400", "FFF0E3"], red: ["D70015", "FFE5E7"],
  purple: ["9B1FB0", "F8E5FB"], indigo: ["4F46E5", "EBE9FF"], teal: ["0B8A94", "E0F6F8"], gray: ["6E6E73", "ECECF0"],
};
const PT = (v) => v * 2; // half-points

// ---- インライン記法 ------------------------------------------------------
// `code`、**太字**、[[キー]]、{{色|語}}（チップ）の 4 つだけを runs に落とす。改行は入れない（段落で分ける）。
function runs(text, base = {}) {
  const out = [];
  const re = /(`[^`]+`|\*\*[^*]+\*\*|\[\[[^\]]+\]\]|\{\{[a-z]+\|[^}]+\}\})/g;
  let last = 0, m;
  while ((m = re.exec(text))) {
    if (m.index > last) out.push(new TextRun({ ...base, text: text.slice(last, m.index) }));
    const tok = m[0];
    if (tok.startsWith("`")) {
      out.push(new TextRun({ ...base, text: tok.slice(1, -1), font: MONO, size: PT((base.size || PT(10.5)) / 2 - 1.2), shading: { type: ShadingType.CLEAR, fill: BG2, color: "auto" } }));
    } else if (tok.startsWith("**")) {
      out.push(new TextRun({ ...base, text: tok.slice(2, -2), bold: true }));
    } else if (tok.startsWith("[[")) {
      out.push(new TextRun({ ...base, text: tok.slice(2, -2), font: MONO, size: PT(9), border: { style: BorderStyle.SINGLE, size: 4, color: SEP, space: 1 } }));
    } else {
      const [color, word] = tok.slice(2, -2).split("|");
      out.push(chip(word, color, base));
    }
    last = m.index + tok.length;
  }
  if (last < text.length) out.push(new TextRun({ ...base, text: text.slice(last) }));
  return out;
}
function chip(word, color, base = {}) {
  const [fg, bg] = CHIP[color] || CHIP.gray;
  return new TextRun({ ...base, text: ` ${word} `, bold: true, color: fg, size: PT(9.5), shading: { type: ShadingType.CLEAR, fill: bg, color: "auto" } });
}

const p = (text, opt = {}) => new Paragraph({ children: runs(text), spacing: { after: mm(2.4) }, ...opt });
const lead = (text) => new Paragraph({ children: runs(text, { size: PT(12.5), color: LABEL2 }), spacing: { after: mm(5), line: 320 } });
const h1 = (num, text, id) => new Paragraph({
  heading: HeadingLevel.HEADING_1, pageBreakBefore: true, keepNext: true,
  children: [
    new TextRun({ text: `${num}`, color: TINT, size: PT(13), bold: true, characterSpacing: 20 }),
    new Bookmark({ id, children: [new TextRun({ text, break: 1 })] }),
  ],
});
const h2 = (text) => new Paragraph({ heading: HeadingLevel.HEADING_2, keepNext: true, children: [new TextRun(text)] });
const bullet = (text) => new Paragraph({ children: runs(text), numbering: { reference: "bullets", level: 0 }, spacing: { after: mm(1.2) } });
let numInstance = 0; // 番号付きは章ごとに instance を分けて 1 から数え直す
const numbered = (items) => { numInstance += 1; return items.map((t) => new Paragraph({ children: runs(t), numbering: { reference: "numbers", level: 0, instance: numInstance }, spacing: { after: mm(1.2) } })); };
// 淡い地の囲み（Apple の grouped background）。色の帯は付けない＝色は操作できる物の印なので静的な注意書きには使わない
const softBox = { top: { style: BorderStyle.SINGLE, size: 36, color: BG2, space: 6 }, bottom: { style: BorderStyle.SINGLE, size: 36, color: BG2, space: 6 }, left: { style: BorderStyle.SINGLE, size: 36, color: BG2, space: 8 }, right: { style: BorderStyle.SINGLE, size: 36, color: BG2, space: 8 } };
const callout = (text) => new Paragraph({
  children: runs(text), spacing: { before: mm(2), after: mm(4), line: 320 },
  shading: { type: ShadingType.CLEAR, fill: BG2, color: "auto" }, border: softBox,
});
const pre = (text) => new Paragraph({
  children: [new TextRun({ text, font: MONO, size: PT(9) })], spacing: { before: mm(1), after: mm(3) },
  shading: { type: ShadingType.CLEAR, fill: BG2, color: "auto" }, border: softBox,
});
const caption = (text) => new Paragraph({ children: [new TextRun({ text, size: PT(9), color: LABEL2 })], spacing: { before: mm(1.2), after: mm(1.5), line: 260 } });

// ---- 画像 ---------------------------------------------------------------
function pngSize(buf) { return { w: buf.readUInt32BE(16), h: buf.readUInt32BE(20) }; }
// keepNext は本文に直接置く図だけ true にする（表の中で付けると Word が行同士を連結して、表ごと次ページへ落ちる）
function figure(name, widthMm, cap, keepNext = false) {
  const buf = fs.readFileSync(path.join(IMG, name));
  const { w, h } = pngSize(buf);
  const width = mmPx(widthMm), height = Math.round((width * h) / w);
  const img = new Paragraph({ children: [new ImageRun({ type: "png", data: buf, transformation: { width, height }, altText: { title: cap || name, description: cap || name, name } })], keepNext, spacing: { before: mm(1), after: 0, line: 240 } }); // 図の行には本文の行間を掛けない
  return cap ? [img, caption(cap)] : [img];
}
const diagram = (id, widthMm = CONTENT_MM, cap = null) => figure(`diagrams/${id}.png`, widthMm, cap, !!cap); // 説明文が無ければ keepNext も無し（連結で章ごと次ページへ落ちるのを防ぐ）

// ---- 表 -----------------------------------------------------------------
const NO_BORDER = { style: BorderStyle.NONE, size: 0, color: "FFFFFF" };
const noBorders = { top: NO_BORDER, bottom: NO_BORDER, left: NO_BORDER, right: NO_BORDER, insideHorizontal: NO_BORDER, insideVertical: NO_BORDER };

// 図と文を横に並べる（罫線無しの 1 行表）。各列は { width: mm | "rest", children: [...] }
function sideBySide(cols) {
  const fixed = cols.reduce((a, c) => a + (c.width === "rest" ? 0 : mm(c.width)), 0);
  const dxa = cols.map((c) => (c.width === "rest" ? CONTENT_W - fixed : mm(c.width)));
  return new Table({
    width: { size: CONTENT_W, type: WidthType.DXA }, columnWidths: dxa, borders: noBorders,
    rows: [new TableRow({ cantSplit: true, children: cols.map((c, i) => new TableCell({
      width: { size: dxa[i], type: WidthType.DXA }, verticalAlign: VerticalAlign.TOP,
      margins: { top: 0, bottom: 0, left: i === 0 ? 0 : mm(3), right: i === cols.length - 1 ? 0 : mm(3) },
      children: c.children,
    })) })],
  });
}

// Apple の一覧の形: 縦罫線なし・行の間は髪の毛の線・見出し行は小さく灰色。rows[0] が見出し（null なら見出し無し）
function table(rows, colMm, fontPt = 9.5, totalW = CONTENT_W) {
  const widths = colMm.map(mm);
  widths[widths.length - 1] += totalW - widths.reduce((a, b) => a + b, 0);
  const hair = { style: BorderStyle.SINGLE, size: 4, color: SEP };
  const hasHeader = rows[0] !== null;
  const body = rows.filter((r) => r);
  return new Table({
    width: { size: totalW, type: WidthType.DXA }, columnWidths: widths,
    borders: { top: NO_BORDER, bottom: hair, left: NO_BORDER, right: NO_BORDER, insideHorizontal: hair, insideVertical: NO_BORDER },
    rows: body.map((cells, r) => new TableRow({
      cantSplit: true, tableHeader: r === 0 && hasHeader,
      children: cells.map((text, c) => new TableCell({
        width: { size: widths[c], type: WidthType.DXA }, margins: { top: mm(1.6), bottom: mm(1.6), left: c === 0 ? 0 : mm(2), right: mm(2) },
        children: [new Paragraph({
          children: runs(text, r === 0 && hasHeader ? { size: PT(8.5), color: LABEL2, bold: true } : { size: PT(fontPt) }),
          spacing: { after: 0, line: 280 },
        })],
      })),
    })),
  });
}
// 見出し行の無い「項目: 内容」の一覧。左列は灰色のチップ。totalMm は表の幅（表の中に入れる時に狭める）
const kv = (rows, keyMm = 34, totalMm = CONTENT_MM) => table([null, ...rows.map(([k, v]) => [`{{gray|${k}}}`, v])], [keyMm, totalMm - keyMm], 9.5, mm(totalMm));

// ---- 本文 ---------------------------------------------------------------
const CHAPTERS = [
  ["c1", "はじめに"], ["c2", "基本の操作"], ["c3", "Claude Code との連携"], ["c4", "チャット"],
  ["c5", "音声入力と読み上げ"], ["c6", "設定画面"], ["c7", "ペットの切り替えと追加"], ["c8", "困ったとき"],
];

const cover = [
  new Paragraph({ spacing: { before: mm(60) }, children: [] }),
  ...figure("pet-idle.png", 64, null),
  new Paragraph({ spacing: { before: mm(12), after: mm(4) }, children: [new TextRun({ text: "pet-claude", bold: true, size: PT(40), characterSpacing: -20 })] }),
  new Paragraph({ spacing: { after: mm(10) }, children: [new TextRun({ text: "Claude Code と一緒に動く、Windows のデスクトップペット", size: PT(15), color: LABEL2 })] }),
  new Paragraph({ children: [new TextRun({ text: "マニュアル  2026 年 10 月 28 日版", size: PT(10), color: LABEL2 })] }),
];

const toc = [
  new Paragraph({ heading: HeadingLevel.HEADING_1, pageBreakBefore: true, children: [new TextRun("目次")] }),
  ...CHAPTERS.map(([id, title], i) => new Paragraph({
    spacing: { after: mm(3.5) },
    children: [
      new TextRun({ text: String(i + 1).padStart(2, "0"), color: TINT, bold: true, size: PT(11) }),
      new TextRun({ text: "   " }),
      new InternalHyperlink({ anchor: id, children: [new TextRun({ text: title, size: PT(13), color: LABEL })] }),
    ],
  })),
];

const c1 = [
  h1("01", "はじめに", "c1"),
  lead("Claude Code の様子に合わせて動く、小さなキャラクター。他の窓の上に浮かび、フォーカスは奪いません。"),
  ...diagram("system", CONTENT_MM, "ペットは Claude Code の hooks から状態を受け取り、答えを返す。チャットは同じ環境の新しいセッションに送る"),
  h2("できること"),
  kv([
    ["知らせる", "{{blue|作業中}} {{green|できた}} {{red|失敗}} {{orange|入力待ち}} をポーズ・吹き出し・声で"],
    ["答える", "承認待ちと質問をカードで受け、「許可」「拒否」や選択肢を返す"],
    ["話す", "チャットと音声入力。答えは吹き出しと声で"],
    ["選ぶ", "ペットは 6 体。大きさも変えられる"],
  ]),
  h2("画面の名前"),
  sideBySide([
    { width: 34, children: figure("pet-bar.png", 24, "バー: 左がチャット、右が音声入力") },
    { width: 54, children: figure("pet-menu.png", 46, "メニュー: 右クリックで出る") },
    { width: 38, children: figure("status-bubble.png", 27, "吹き出し: 短い知らせ。3 秒で消える") },
    { width: "rest", children: figure("perm-card.png", 36, "承認のカード: Claude Code の承認待ち") },
  ]),
  p("このほかに、バーから開く**チャット欄**（4 章）と、メニューから開く**設定画面**（6 章）があります。", { spacing: { before: mm(2), after: 0 } }),
];

const c2 = [
  h1("02", "基本の操作", "c2"),
  h2("起動と終了"),
  kv([
    ["自動で出る", "Windows タスク `pet-claude` がサインインの 10 秒後に出す（設定「サインインしたらペットを出す」）"],
    ["手で出す", "`pet-app.exe` を起動。`pet-hook.exe` と同じフォルダに置く。設定画面を直接開くなら `--open-settings`"],
    ["二重起動", "しない。すでに出ていれば新しい方が静かに終わる"],
    ["終わる", "メニューかトレイのアイコンの「終了」"],
  ]),
  h2("マウス"),
  ...diagram("mouse"),
  h2("放っておいたとき"),
  ...diagram("idle"),
  callout("**ペットが見当たらない** → トレイのアイコンの「位置をリセット」。手前の画面の決まった場所に戻ります。"),
];

const c3 = [
  h1("03", "Claude Code との連携", "c3"),
  lead("Claude Code の hooks が `pet-hook.exe` を呼び、ペットに状態を伝えます。"),
  p("登録は `pet-app.exe --install-hooks`、解除は `--uninstall-hooks`。exe を移したら登録し直します。"),
  h2("ペットが知らせる状態"),
  ...diagram("states", 150),
  h2("承認のカード"),
  sideBySide([
    { width: 132, children: diagram("approve", 128) },
    { width: "rest", children: figure("ask-card.png", 26, "質問のカード（下）") },
  ]),
  h2("質問のカード"),
  kv([
    ["選ぶ", "選択肢を押す。複数選択は選んでから「決定」"],
    ["書く", "「回答を書く…」でチャット欄が答えの入力になる"],
    ["複数", "1 問ずつ進み、全部答えてから Claude Code に返す"],
    ["オフ", "設定「質問にペットで答える」をオフにすると「聞きたいことがあるよ」と知らせるだけ"],
  ], 20),
];

const c4 = [
  h1("04", "チャット", "c4"),
  lead("ペットから Claude に話しかける。送り先は Claude Code と同じ環境（設定・CLAUDE.md・スキル・hooks・MCP）の新しいセッション。接続と作業フォルダは設定画面で変える（6 章）。"),
  ...diagram("chat"),
  h2("送る"),
  sideBySide([
    { width: 76, children: [...figure("chat-empty.png", 70, "チャット欄。上が入力、下の行がモードとモデル・エフォート"), ...figure("chat-answer.png", 44, "答えが出たところ。右上の「新しいチャット」で最初から")] },
    { width: "rest", children: [
      ...numbered([
        "バーの左のボタン（鉛筆）を押す。",
        "文を打って [[Enter]] か矢印ボタンで送る。",
        "答えは吹き出しに 1 行、チャット欄に全文。",
        "続けて打つと同じ会話。「新しいチャット」で忘れる。",
      ]),
      kv([
        ["閉じる", "ペットを左クリック。[[Esc]] や外のクリックでは閉じない（日本語入力の取り消しに使うため）"],
      ], 18, 90),
    ] },
  ]),
  h2("モード・モデル・エフォート"),
  sideBySide([
    { width: 40, children: figure("chat-menu-mode.png", 34, "左の「自動」→ モードの一覧") },
    { width: 40, children: figure("chat-menu-model.png", 34, "右の「Fable 5.1 高」→ モデル。「エフォート ›」で強さ") },
    { width: "rest", children: [kv([
      ["選ぶ", "押すとメニューが上に出る。数字キーでも選べる"],
      ["効く範囲", "そのチャットの間だけ。「新しいチャット」で設定画面の既定に戻る"],
      ["承認", "同じ hooks が効くので承認カードが出る"],
      ["書かせる", "{{blue|編集を受け入れる}} にすると編集で止まらない"],
    ], 20, 91)] },
  ]),
];

const c5 = [
  h1("05", "音声入力と読み上げ", "c5"),
  h2("音声入力（話してチャットに送る）"),
  p("初回だけ `scripts\\setup-stt.ps1` を PowerShell で実行（文字起こしの環境と faster-whisper small を用意）。設定「音声入力」が「使えます」になれば準備完了。"),
  ...diagram("voice"),
  kv([
    ["聞き取った文", "チャット欄の答えの上に「」付きで出る"],
    ["ミュート", "「マイクがミュートになっているよ」→ Windows のサウンド設定か、キーボードのミュートキーを見る"],
  ]),
  h2("読み上げ（声で知らせる）"),
  p("設定「声で知らせる」をオン。声は AivisSpeech の天深シノ。"),
  kv([
    ["読む文", "チャットの答え（吹き出しの 1 行）と、固定の文 {{green|できたよ}} {{red|失敗しちゃった}} {{orange|入力を待ってるよ}} {{purple|確認してほしいことがあるよ}} {{indigo|聞きたいことがあるよ}}"],
    ["繰り返し", "同じ文は 15 秒以内に繰り返さない"],
    ["エンジン", "動いていなければペットが起こす（20 秒ほど）。10 分話さなければ止める（約 3.4GB のメモリを使うため）"],
    ["場所", "`%USERPROFILE%\\dev\\voice\\_engines\\AivisSpeech\\AivisSpeech-Engine\\run.exe`。無ければ設定「読み上げ」に「エンジンが見つかりません」と出て黙る"],
    ["読まない答え", "URL・パス・コードらしい文字列が入ると、吹き出しには出さず「答えはチャット欄に出したよ」"],
  ]),
];

const c6 = [
  h1("06", "設定画面", "c6"),
  lead("メニューかトレイのアイコンの「設定」。項目を変えるとすぐ効き、保存ボタンはありません。"),
  sideBySide([
    { width: 36, children: figure("settings.png", 30, "設定画面") },
    { width: "rest", children: [table([
      ["節", "項目", "内容"],
      ["{{blue|表示}}", "ペット", "使うペットを選ぶ。位置と大きさはそのまま"],
      ["", "ウェイクワード", "呼びかけの言葉を見るだけ（呼びかけで音声入力を始める機能は保留中）"],
      ["", "ペットの大きさ", "25〜150%。既定 55%"],
      ["", "放置すると散歩する", "既定オフ。眠るのは設定に関係なく眠る"],
      ["{{blue|起動}}", "サインインしたらペットを出す", "Windows タスク `pet-claude` を登録。開くたびに実物を読み直す"],
      ["{{blue|Claude Code}}", "質問にペットで答える", "既定オン。オフだと「聞きたいことがあるよ」だけ"],
      ["{{blue|声}}", "声で知らせる", "読み上げのオン・オフ"],
      ["", "読み上げ", "AivisSpeech の状態（使えます／話す時に起こします／エンジンが見つかりません）"],
      ["", "音声入力", "文字起こしの状態（使えます／準備が要ります／マイクが見つかりません／ミュート）"],
      ["{{blue|チャット}}", "接続", "サブスクか API キー。API キーは Windows の資格情報マネージャーに保存"],
      ["", "いまの接続", "「確かめる」で接続を試す（15 秒以内）"],
      ["", "モデル", "Opus 5.5・Fable 5.1・Sonnet 5.5・Haiku 5.5。既定 Fable 5.1"],
      ["", "エフォート", "低・中・高・最高・最大。既定 高"],
      ["", "モード", "自動・手動・編集を受け入れる・プラン・権限をバイパス。既定 自動"],
      ["", "作業フォルダ", "既定はホーム。「選ぶ」で変え、「既定に戻す」で戻す"],
    ], [24, 38, 73], 9, mm(135))] },
  ]),
];

const c7 = [
  h1("07", "ペットの切り替えと追加", "c7"),
  lead("設定「ペット」で選ぶ。まだ選んでいなければ、まめドラゴンで起動。"),
  ...diagram("pets"),
  h2("足す"),
  kv([
    ["置く場所", "`%LOCALAPPDATA%\\pet-claude\\pets\\<id>\\`（`pet.json` とスプライトシート）。次に設定画面を開いたときに一覧に出る"],
    ["静止画から", "状態ごとの PNG か WebP（ファイル名に `idle` `walk` `working` `attention` `happy` `confused` `sleeping` `dragged`）があれば下のコマンドで作れる。足りない状態は近い絵で埋める"],
    ["動画つき", "同梱の 6 体の作り方は `.claude/skills/make-pet-clips/` と各ペットの `SOURCE.md`"],
  ]),
  pre("cargo run --release -p pet-core --example build_pet -- <静止画のフォルダ> <出力フォルダ> <id> <表示名>"),
];

const c8 = [
  h1("08", "困ったとき", "c8"),
  table([
    ["症状", "やること"],
    ["ペットが出ない", "`%LOCALAPPDATA%\\pet-claude\\logs\\pet.log` の末尾を見る。「既に起動しているので終了」ならもう出ている → 「位置をリセット」"],
    ["起動時にダイアログ", "ペットを 1 つも読めない。`pets\\<id>\\pet.json` の場所と中身を確かめる（`--pet` で指定したときも同じ）"],
    ["承認のカードが出ない", "`pet-app.exe --install-hooks` で登録し直す。`pet-hook.exe` が同じフォルダにあるか見る。ログに `permission requested` が無ければ hook が届いていない"],
    ["チャットが失敗", "チャット欄に理由が出る。設定「いまの接続」の「確かめる」。`claude` が PATH にあるか、ログインが切れていないか"],
    ["声が出ない", "設定「読み上げ」の状態を見る。エンジンの場所は 5 章。Windows の出力デバイスも見る"],
    ["音声入力が動かない", "設定「音声入力」の状態を見る。「準備が要ります」なら `scripts\\setup-stt.ps1`"],
    ["設定を初期化", "ペットを終了して `%LOCALAPPDATA%\\pet-claude\\state.json` を消す（API キーは資格情報マネージャーに残る）"],
  ], [40, 134]),
  callout("ログには本文も答えも鍵も書きません。設定ファイルにも鍵は入りません。詳しい設計は `docs/architecture.md`、手順の一覧は `docs/usage.md`。"),
];

// ---- 文書 ---------------------------------------------------------------
const doc = new Document({
  creator: "pet-claude", title: "pet-claude マニュアル", description: "Claude Code と一緒に動く Windows のデスクトップペットの使い方",
  styles: {
    default: { document: { run: { font: { ascii: FONT, hAnsi: FONT, eastAsia: FONT, cs: FONT }, size: PT(10.5), color: LABEL }, paragraph: { spacing: { line: 330 } } } },
    paragraphStyles: [
      // Normal を明示する。無いと日本語の Word が自前の既定（游明朝＝テーマの本文フォント）で補い、docDefaults の指定が負ける（2026-10-28 実測）
      { id: "Normal", name: "Normal", quickFormat: true,
        run: { font: { ascii: FONT, hAnsi: FONT, eastAsia: FONT, cs: FONT }, size: PT(10.5), color: LABEL, language: { value: "ja-JP", eastAsia: "ja-JP" } },
        paragraph: { spacing: { line: 330 } } },
      // 章: 大きな文字で階層を作る（罫線は引かない）。章番号は tint の小さな文字
      { id: "Heading1", name: "Heading 1", basedOn: "Normal", next: "Normal", quickFormat: true,
        run: { size: PT(26), bold: true, color: LABEL, font: { ascii: FONT, hAnsi: FONT, eastAsia: FONT }, characterSpacing: -10 },
        paragraph: { spacing: { before: 0, after: mm(6), line: 260 }, outlineLevel: 0 } },
      { id: "Heading2", name: "Heading 2", basedOn: "Normal", next: "Normal", quickFormat: true,
        run: { size: PT(14), bold: true, color: LABEL, font: { ascii: FONT, hAnsi: FONT, eastAsia: FONT } },
        paragraph: { spacing: { before: mm(7), after: mm(2.5) }, outlineLevel: 1 } },
    ],
  },
  numbering: {
    config: [
      { reference: "bullets", levels: [{ level: 0, format: LevelFormat.BULLET, text: "•", alignment: AlignmentType.LEFT, style: { paragraph: { indent: { left: mm(5.5), hanging: mm(3.5) } } } }] },
      { reference: "numbers", levels: [{ level: 0, format: LevelFormat.DECIMAL, text: "%1.", alignment: AlignmentType.LEFT, style: { paragraph: { indent: { left: mm(6), hanging: mm(4) } } } }] },
    ],
  },
  sections: [{
    properties: { page: { size: { width: PAGE_W, height: PAGE_H }, margin: MARGIN } },
    footers: { default: new Footer({ children: [new Paragraph({ alignment: AlignmentType.CENTER, children: [
      new TextRun({ text: "pet-claude マニュアル  ", size: PT(8), color: LABEL2 }),
      new TextRun({ children: [PageNumber.CURRENT], size: PT(8), color: LABEL2 }),
      new TextRun({ text: " / ", size: PT(8), color: LABEL2 }),
      new TextRun({ children: [PageNumber.TOTAL_PAGES], size: PT(8), color: LABEL2 }),
    ] })] }) },
    children: [...cover, ...toc, ...c1, ...c2, ...c3, ...c4, ...c5, ...c6, ...c7, ...c8],
  }],
});

Packer.toBuffer(doc).then((buf) => {
  fs.writeFileSync(OUT, buf);
  console.log("wrote", OUT, buf.length, "bytes");
});
