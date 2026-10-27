// マニュアルの初版（pet-claude-manual.docx）を docx（npm）で組む。
// 使い方: node docs/manual/build-docx.js [出力パス]  → 既定は docs/manual/pet-claude-manual.docx
//
// 正本は docx の方（K が Word で直す）。このスクリプトは最初の 1 回と、構成を丸ごと作り直す時にだけ使う。
// 走らせると K の手直しを上書きするので、普段は走らせない（docs/manual/README.md）。
// PDF は export-pdf.ps1 で docx から書き出す。
//
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
const MARGIN = { top: mm(18), right: mm(16), bottom: mm(20), left: mm(16) };
const CONTENT_W = PAGE_W - MARGIN.left - MARGIN.right; // 10,092 DXA

// ---- 字体と色 -----------------------------------------------------------
// 本文は Windows 11 に入っている Yu Gothic UI（検証機で実在を確認）。等幅は Cascadia Mono（無ければ Word が Consolas に落とす）。
const FONT = "Yu Gothic UI";
const MONO = { ascii: "Cascadia Mono", hAnsi: "Cascadia Mono", cs: "Cascadia Mono", eastAsia: "Yu Gothic UI" };
const INK = "1D1D1F", INK2 = "5C5C62", LINE = "D9D9DE", TINT = "0066CC", BG2 = "F4F4F7";
const PT = (v) => v * 2; // half-points

// ---- インライン記法 ------------------------------------------------------
// `code`、**太字**、[[キー]] の 3 つだけを runs に落とす。改行は入れない（段落で分ける）。
function runs(text, base = {}) {
  const out = [];
  const re = /(`[^`]+`|\*\*[^*]+\*\*|\[\[[^\]]+\]\])/g;
  let last = 0, m;
  while ((m = re.exec(text))) {
    if (m.index > last) out.push(new TextRun({ ...base, text: text.slice(last, m.index) }));
    const tok = m[0];
    if (tok.startsWith("`")) {
      out.push(new TextRun({ ...base, text: tok.slice(1, -1), font: MONO, size: PT(9.2), shading: { type: ShadingType.CLEAR, fill: BG2, color: "auto" } }));
    } else if (tok.startsWith("**")) {
      out.push(new TextRun({ ...base, text: tok.slice(2, -2), bold: true }));
    } else {
      out.push(new TextRun({ ...base, text: tok.slice(2, -2), font: MONO, size: PT(9), border: { style: BorderStyle.SINGLE, size: 4, color: LINE, space: 1 } }));
    }
    last = m.index + tok.length;
  }
  if (last < text.length) out.push(new TextRun({ ...base, text: text.slice(last) }));
  return out;
}

const p = (text, opt = {}) => new Paragraph({ children: runs(text), spacing: { after: mm(2.6) }, ...opt });
const h1 = (text, id) => new Paragraph({
  heading: HeadingLevel.HEADING_1, pageBreakBefore: true, keepNext: true,
  children: [new Bookmark({ id, children: [new TextRun(text)] })],
});
const h2 = (text) => new Paragraph({ heading: HeadingLevel.HEADING_2, keepNext: true, children: [new TextRun(text)] });
const bullet = (text, level = 0) => new Paragraph({ children: runs(text), numbering: { reference: "bullets", level }, spacing: { after: mm(1.2) } });
// 番号付きは章ごとに instance を分けて 1 から数え直す
let numInstance = 0;
const numbered = (items) => { numInstance += 1; return items.map((t) => new Paragraph({ children: runs(t), numbering: { reference: "numbers", level: 0, instance: numInstance }, spacing: { after: mm(1.2) } })); };
const note = (text) => new Paragraph({
  children: runs(text), spacing: { before: mm(1), after: mm(3.5) }, indent: { left: mm(1) },
  shading: { type: ShadingType.CLEAR, fill: BG2, color: "auto" },
  border: { left: { style: BorderStyle.SINGLE, size: 24, color: TINT, space: 8 } },
});
const pre = (text) => new Paragraph({
  children: [new TextRun({ text, font: MONO, size: PT(9) })], spacing: { before: mm(1), after: mm(3) },
  shading: { type: ShadingType.CLEAR, fill: BG2, color: "auto" }, indent: { left: mm(2), right: mm(2) },
});
const caption = (text) => new Paragraph({ children: [new TextRun({ text, size: PT(9), color: INK2 })], spacing: { before: mm(1.2), after: mm(1.5) } });

// ---- 画像 ---------------------------------------------------------------
function pngSize(buf) { return { w: buf.readUInt32BE(16), h: buf.readUInt32BE(20) }; }
// keepNext は本文に直接置く図だけ true にする（表の中で付けると Word が行同士を連結して、表ごと次ページへ落ちる）
function figure(name, widthMm, cap, keepNext = false) {
  const buf = fs.readFileSync(path.join(IMG, name));
  const { w, h } = pngSize(buf);
  const width = mmPx(widthMm), height = Math.round((width * h) / w);
  const img = new Paragraph({ children: [new ImageRun({ type: "png", data: buf, transformation: { width, height }, altText: { title: cap, description: cap, name } })], keepNext, spacing: { before: mm(1), after: 0, line: 240 } });  // 図の行には本文の行間（1.42 倍）を掛けない
  return cap ? [img, caption(cap)] : [img];
}

// ---- 表 -----------------------------------------------------------------
const NO_BORDER = { style: BorderStyle.NONE, size: 0, color: "FFFFFF" };
const noBorders = { top: NO_BORDER, bottom: NO_BORDER, left: NO_BORDER, right: NO_BORDER, insideHorizontal: NO_BORDER, insideVertical: NO_BORDER };
const cellMargins = { top: mm(1.2), bottom: mm(1.2), left: mm(2), right: mm(2) };

// 図と文を横に並べる（罫線無しの 1 行表）。各列は { width: mm, children: [...] }
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

// 見出し行つきの表。rows[0] が見出し。colMm は列幅（mm）、合計が本文幅を超えないこと
function table(rows, colMm, fontPt = 9.3) {
  const widths = colMm.map(mm);
  const sum = widths.reduce((a, b) => a + b, 0);
  widths[widths.length - 1] += CONTENT_W - sum; // 端数は最後の列へ
  const edge = { style: BorderStyle.SINGLE, size: 4, color: LINE };
  return new Table({
    width: { size: CONTENT_W, type: WidthType.DXA }, columnWidths: widths,
    borders: { top: edge, bottom: edge, left: NO_BORDER, right: NO_BORDER, insideHorizontal: edge, insideVertical: NO_BORDER },
    rows: rows.map((cells, r) => new TableRow({
      cantSplit: true, tableHeader: r === 0,
      children: cells.map((text, c) => new TableCell({
        width: { size: widths[c], type: WidthType.DXA }, margins: cellMargins,
        shading: r === 0 ? { type: ShadingType.CLEAR, fill: BG2, color: "auto" } : undefined,
        children: [new Paragraph({ children: runs(text, { size: PT(fontPt), bold: r === 0 }), spacing: { after: 0 } })],
      })),
    })),
  });
}

// ---- 本文 ---------------------------------------------------------------
const CHAPTERS = [
  ["c1", "1. はじめに"], ["c2", "2. 基本の操作"], ["c3", "3. Claude Code との連携"], ["c4", "4. チャット"],
  ["c5", "5. 音声入力と読み上げ"], ["c6", "6. 設定画面"], ["c7", "7. ペットの切り替えと追加"], ["c8", "8. 困ったとき"],
];

const cover = [
  new Paragraph({ spacing: { before: mm(70) }, children: [] }),
  ...figure("pet-idle.png", 60, null),
  new Paragraph({ spacing: { before: mm(10), after: mm(6) }, children: [new TextRun({ text: "pet-claude マニュアル", bold: true, size: PT(34) })] }),
  new Paragraph({ spacing: { after: mm(14) }, children: [new TextRun({ text: "Claude Code と一緒に動く、Windows のデスクトップペット", size: PT(13), color: INK2 })] }),
  new Paragraph({ children: runs("2026 年 10 月 28 日版。手順だけを読みたいときは `docs/usage.md`、設計は `docs/architecture.md`。", { size: PT(9.5), color: INK2 }) }),
];

const toc = [
  new Paragraph({ heading: HeadingLevel.HEADING_1, pageBreakBefore: true, children: [new TextRun("目次")] }),
  ...CHAPTERS.map(([id, title]) => new Paragraph({
    spacing: { after: mm(2.2) },
    children: [new InternalHyperlink({ anchor: id, children: [new TextRun({ text: title, size: PT(11.5), color: INK })] })],
  })),
];

const c1 = [
  h1("1. はじめに", "c1"),
  p("pet-claude は、Claude Code の様子に合わせて動く小さなキャラクターをデスクトップに出すアプリです。他のウィンドウの上に浮かび、フォーカスは奪いません。Claude Code が承認や回答を待っていると吹き出しで知らせ、ペットから答えを返せます。チャットと音声入力もペットから使えます。"),
  h2("できること"),
  bullet("Claude Code の状態（作業中・入力待ち・完了・失敗）に合わせてポーズが変わり、吹き出しと声で知らせる"),
  bullet("承認待ちと質問を吹き出しのカードで受け、「許可」「拒否」や選択肢を返す"),
  bullet("チャット: Claude Code と同じ環境の新しいセッションに送り、答えを吹き出しと声で返す"),
  bullet("音声入力（ローカルで文字起こし）と読み上げ（AivisSpeech）"),
  bullet("ペットは 6 体から選ぶ。大きさも変えられる"),
  h2("画面の名前"),
  p("このマニュアルでは、ペットの周りに出る部品を次の名前で呼びます。このほかに、バーのチャットから開く**チャット欄**（4 章）と、メニューから開く**設定画面**（6 章）があります。"),
  sideBySide([
    { width: 58, children: figure("pet-bar.png", 24, "バー: クリックかホバーでペットの下に出る。左がチャット、右が音声入力") },
    { width: "rest", children: figure("pet-menu.png", 46, "メニュー: 右クリックで出る（設定・バーを隠す・位置をリセット・終了）") },
  ]),
  sideBySide([
    { width: 58, children: figure("status-bubble.png", 27, "吹き出し: 「できたよ」などの短い知らせ。3 秒で消える") },
    { width: "rest", children: figure("perm-card.png", 36, "承認のカード: Claude Code の承認待ちを吹き出しに出す") },
  ]),
];

const c2 = [
  h1("2. 基本の操作", "c2"),
  h2("起動と終了"),
  bullet("普段は Windows タスク `pet-claude` が、サインインの 10 秒後にペットを出します（設定「サインインしたらペットを出す」がオンのとき）。"),
  bullet("手で出すときは `pet-app.exe` を起動します。`pet-hook.exe` と同じフォルダに置いておきます。"),
  bullet("設定画面をすぐ開きたいときは `pet-app.exe --open-settings`。"),
  bullet("二重には起動しません。すでに出ているときは、新しい方が静かに終わります。"),
  bullet("終わるときは、メニューかトレイのアイコンの「終了」。"),
  h2("マウスの操作"),
  table([
    ["操作", "何が起きるか"],
    ["左クリック", "ペットがなでられた反応をし、下にバーが出る。もう一度クリックすると閉じる"],
    ["カーソルを乗せて 0.2 秒", "同じバーが出る。ペットとバーから離れて 0.4 秒で閉じる。クリックすると固定される"],
    ["右クリック", "メニュー（設定・バーを隠す・位置をリセット・終了）"],
    ["ドラッグ", "つままれたポーズで動く。離すと置かれ、画面の外には出ない。位置は画面ごとに覚える"],
  ], [42, 136]),
  h2("放っておいたとき"),
  bullet("設定「放置すると散歩する」がオンなら、90 秒ほど何もしないと散歩を始めます（既定はオフ）。"),
  bullet("5 分ほどで眠ります。Claude Code が動き出すか、クリックすると起きます。"),
  bullet("全画面のアプリを使っている間は前に出ません。閉じると戻ります。"),
  note("ペットが見当たらないときは、トレイのアイコンの「位置をリセット」で、手前の画面の決まった場所に戻ります。"),
];

const c3 = [
  h1("3. Claude Code との連携", "c3"),
  p("Claude Code の hooks から `pet-hook.exe` が呼ばれ、ペットに状態を伝えます。hooks は `~/.claude/settings.json` に登録済みです。登録し直すときは `pet-app.exe --install-hooks`、外すときは `--uninstall-hooks`。`pet-app.exe` を別の場所に移したら登録し直します。"),
  h2("状態の知らせ"),
  bullet("Claude Code が動き出すと、ペットが作業のポーズになります。"),
  bullet("応答が終わると 1 回喜んで「できたよ」。失敗すると「失敗しちゃった」。"),
  bullet("入力を待っていると「入力を待ってるよ」。"),
  h2("承認のカード"),
  sideBySide([
    { width: 46, children: figure("perm-card.png", 40, "ツール名と要約。「ターミナルで」「拒否」「許可」") },
    { width: "rest", children: [
      bullet("Claude Code がツールの承認を待つと、ペットがこちらを見て、カードが出ます。"),
      bullet("「許可」か「拒否」を押すと、Claude Code はそのまま進みます。"),
      bullet("「ターミナルで」を押すと、いつもの画面で答える流れに戻ります。デスクトップアプリから来た要求では「Claude アプリで」になります。"),
      bullet("「許可」が無いカードは、要約が操作の全部を見せていないときです（ファイルの本文を書く Write や Edit、長いコマンドなど）。そのときはいつもの画面で答えます。"),
      bullet("押さずに 1 分ほど放置すると、カードは下がり、Claude Code のいつもの流れに戻ります。"),
      bullet("同時に複数来たときは古い順に 1 枚ずつ出ます（「ほかに N 件」）。"),
    ] },
  ]),
  h2("質問のカード"),
  sideBySide([
    { width: 46, children: figure("ask-card.png", 40, "選択肢を押して答える。「回答を書く…」で自由に書ける") },
    { width: "rest", children: [
      bullet("Claude Code が質問（`AskUserQuestion`）をすると、質問のカードが出ます。"),
      bullet("選択肢を押すと答えになります。複数選択の質問は、選んでから「決定」。"),
      bullet("「回答を書く…」を押すと、チャット欄が答えの入力に変わります。"),
      bullet("質問が複数あるときは 1 問ずつ進み、全部答えてから Claude Code に返します。"),
      bullet("設定「質問にペットで答える」をオフにすると、カードは出さず「聞きたいことがあるよ」とだけ知らせます。"),
    ] },
  ]),
];

const c4 = [
  h1("4. チャット", "c4"),
  p("ペットから Claude に話しかけられます。送り先は Claude Code の対話と同じ環境（設定・CLAUDE.md・スキル・hooks・MCP）の新しいセッションです。送るたびに `claude -p` を起こすので、答えまで 20 秒ほどかかることがあります。"),
  h2("送る"),
  sideBySide([
    { width: 81, children: figure("chat-empty.png", 75, "チャット欄。上が入力、下の行がモードとモデル・エフォート") },
    { width: "rest", children: numbered([
      "バーの左のボタン（鉛筆）を押すと、ペットの下にチャット欄が開きます。",
      "文を打って [[Enter]] か右の矢印ボタンで送ります（2000 字まで）。",
      "答えは吹き出しに 1 行、チャット欄の上に全文が出ます。吹き出しに収まらないときは「続きを見る」で開きます。",
      "続けて打つと同じ会話の続きです。「新しいチャット」を押すと会話を忘れます。",
    ]) },
  ]),
  ...figure("chat-answer.png", 40, "答えが出たところ。右上の「新しいチャット」で最初から", true),
  h2("モード・モデル・エフォートを切り替える"),
  sideBySide([
    { width: 46, children: figure("chat-menu-mode.png", 40, "左の「自動」を押すとモードの一覧") },
    { width: "rest", children: figure("chat-menu-model.png", 40, "右の「Fable 5.1 高」を押すとモデルの一覧。「エフォート ›」で強さ") },
  ]),
  bullet("チャット欄の下の行で、左がモード（権限の扱い）、右がモデルとエフォートです。押すとメニューが上に出ます。数字キーでも選べます。"),
  bullet("ここで変えた値はそのチャットの間だけ効き、「新しいチャット」で設定画面の既定に戻ります。"),
  bullet("同じ hooks が効くので、チャットから承認待ちになるとペットの承認カードが出ます。ファイルを書かせる作業は、モードを「編集を受け入れる」にすると編集で止まらなくなります。"),
  h2("閉じる"),
  bullet("チャット欄は、ペットを左クリックすると閉じます。"),
  bullet("[[Esc]] や外のクリックでは閉じません（日本語入力の取り消しに Esc を使うため）。"),
  h2("接続と作業フォルダ"),
  bullet("接続は設定「接続」で選びます。既定はサブスク（Claude Code にログインしているアカウント）。API キーを選ぶと使った分だけ課金されます。"),
  bullet("設定「いまの接続」の「確かめる」で、いまの接続で答えられるかを調べます。"),
  bullet("作業フォルダは設定「作業フォルダ」。既定はホームで、選んだフォルダの CLAUDE.md と `.claude` も読みます。"),
];

const c5 = [
  h1("5. 音声入力と読み上げ", "c5"),
  h2("音声入力（話してチャットに送る）"),
  p("初回だけ、リポジトリの `scripts\\setup-stt.ps1` を PowerShell で 1 回実行します（文字起こしの Python 環境と faster-whisper small を用意します）。設定画面「音声入力」の行が「使えます」になれば準備できています。"),
  sideBySide([
    { width: 46, children: figure("pet-bar.png", 40, "右の波形のボタンが音声入力") },
    { width: "rest", children: [
      ...numbered([
        "バーの右のボタン（波形）を押すと録音が始まり、ボタンが赤く脈打ちます。",
        "話し終えて 1 秒黙ると自動で止まり、文字起こしを経てチャットに送ります。",
        "もう一度押すと、その場で止めて送ります。",
        "5 秒以内に話し始めないと取りやめます。1 回の録音は 15 秒までです。",
      ]),
      bullet("聞き取った文は、チャット欄の答えの上に「」付きで出ます。"),
      bullet("マイクがミュートだと「マイクがミュートになっているよ」。Windows のサウンドの設定か、キーボードのミュートキーを見ます。"),
      bullet("録音はファイルに残しません。"),
    ] },
  ]),
  h2("読み上げ（声で知らせる）"),
  p("設定「声で知らせる」をオンにします。声は AivisSpeech の天深シノです。"),
  bullet("読むのは、チャットの答え（吹き出しの 1 行）と固定の文（できたよ・失敗しちゃった・入力を待ってるよ・確認してほしいことがあるよ・聞きたいことがあるよ）。"),
  bullet("同じ文は 15 秒以内に繰り返しません。"),
  bullet("AivisSpeech エンジンが動いていなければペットが起こします（使えるまで 20 秒ほど）。ペットが起こしたエンジンは、10 分話さなければ止めます（約 3.4GB のメモリを使うため）。"),
  bullet("エンジンの場所は `%USERPROFILE%\\dev\\voice\\_engines\\AivisSpeech\\AivisSpeech-Engine\\run.exe`。無いときは設定画面「読み上げ」の行に「エンジンが見つかりません」と出て、黙ります。"),
  bullet("答えに URL やパス、コードらしい文字列が入っていると、吹き出しには出さず「答えはチャット欄に出したよ」と言います。"),
];

const c6 = [
  h1("6. 設定画面", "c6"),
  p("メニューかトレイのアイコンの「設定」で開きます。項目を変えるとすぐ効き、保存ボタンはありません。"),
  sideBySide([
    { width: 36, children: figure("settings.png", 30, "設定画面（上から 表示・起動・Claude Code・声・チャット）") },
    { width: "rest", children: [table([
      ["項目", "内容"],
      ["ペット", "使うペットを選ぶ。足元の位置と大きさはそのまま"],
      ["ウェイクワード", "選んだペットの呼びかけの言葉を見るだけ（呼びかけで音声入力を始める機能は保留中）"],
      ["ペットの大きさ", "25〜150%。既定 55%"],
      ["放置すると散歩する", "既定オフ。眠るのは設定に関係なく眠る"],
      ["サインインしたらペットを出す", "Windows タスク `pet-claude` を登録する。開くたびにタスクの実物を読み直す"],
      ["質問にペットで答える", "既定オン。オフだと「聞きたいことがあるよ」と知らせるだけ"],
      ["声で知らせる", "読み上げのオン・オフ"],
      ["読み上げ", "AivisSpeech の状態（使えます／エンジンは話す時に起こします／エンジンが見つかりません）"],
      ["音声入力", "文字起こしの状態（使えます／準備が要ります／マイクが見つかりません／ミュート）"],
    ], [44, 92], 9)] },
  ]),
  h2("チャットの節"),
  table([
    ["項目", "内容"],
    ["接続", "サブスクか API キー。API キーは Windows の資格情報マネージャーに保存し、画面には読み返さない"],
    ["いまの接続", "「確かめる」で接続を試す（15 秒以内）"],
    ["モデル", "チャットの既定。Opus 5.5・Fable 5.1・Sonnet 5.5・Haiku 5.5。既定 Fable 5.1"],
    ["エフォート", "低・中・高・最高・最大。既定 高"],
    ["モード", "権限モード。自動・手動・編集を受け入れる・プラン・権限をバイパス。既定 自動"],
    ["作業フォルダ", "Claude Code を起こす場所。既定はホーム。「選ぶ」で変え、「既定に戻す」で戻す"],
  ], [32, 146], 9),
];

const c7 = [
  h1("7. ペットの切り替えと追加", "c7"),
  h2("切り替える"),
  bullet("設定「ペット」で選びます。同梱は 6 体: まめドラゴン、りんりん（パンダ）、とらちゃん（茶トラ）、しばちゃん（黒柴）、こむぎちゃん（ハムスター茶）、ごまちゃん（ハムスター灰）。"),
  bullet("まだ選んでいなければ、まめドラゴンで起動します。"),
  bullet("選んだペットの呼びかけの言葉が「ウェイクワード」の行に出ます。呼びかけで音声入力を始める機能はいまは保留中で、表示だけです。"),
  h2("足す"),
  bullet("ペットは 1 つのフォルダ（`pet.json` とスプライトシート）です。`%LOCALAPPDATA%\\pet-claude\\pets\\<id>\\` に置くと、次に設定画面を開いたときに一覧に出ます。"),
  bullet("状態ごとの静止画（ファイル名に `idle` `walk` `working` `attention` `happy` `confused` `sleeping` `dragged` を含む PNG か WebP）があれば、リポジトリで次のコマンドを実行するとペットのフォルダができます。足りない状態は近い絵で埋めます。"),
  pre("cargo run --release -p pet-core --example build_pet -- <静止画のフォルダ> <出力フォルダ> <id> <表示名>"),
  bullet("姿勢の間を動画でつなぐペット（同梱の 6 体はこの形）の作り方は、プロジェクトスキル `.claude/skills/make-pet-clips/` と、各ペットのフォルダの `SOURCE.md` にあります。"),
];

const c8 = [
  h1("8. 困ったとき", "c8"),
  table([
    ["症状", "見るところ・やること"],
    ["ペットが出ない", "ログ `%LOCALAPPDATA%\\pet-claude\\logs\\pet.log` の末尾を見る。「既に起動しているので終了」ならもう出ている（別の画面か、全画面アプリの裏）。トレイのアイコンの「位置をリセット」で手前の画面に戻る"],
    ["起動時にダイアログが出る", "ペットを 1 つも読めない。`pets\\<id>\\pet.json` の場所と中身を確かめる。`--pet` で指定したフォルダが読めないときも同じ"],
    ["承認のカードが出ない", "`pet-app.exe --install-hooks` を実行して登録を確かめる。`pet-hook.exe` が `pet-app.exe` と同じフォルダにあるか見る。ログに `permission requested` が無ければ hook が届いていない"],
    ["チャットが「うまくいきませんでした」", "チャット欄に理由が出る。設定「いまの接続」の「確かめる」を押す。`claude` が PATH にあるか、ログインが切れていないかを見る"],
    ["声が出ない", "設定「読み上げ」の行の状態を見る。エンジンの場所が違うなら 5 章のパスに置く。Windows の出力デバイスも見る"],
    ["音声入力が動かない", "設定「音声入力」の行の状態を見る。「準備が要ります」なら `scripts\\setup-stt.ps1` を実行する"],
    ["設定を最初からやり直したい", "ペットを終了してから `%LOCALAPPDATA%\\pet-claude\\state.json` を消す（大きさ・位置・ペットの選択・チャットの既定が初期値に戻る。API キーは資格情報マネージャーに残る）"],
  ], [44, 134]),
  p("ログには本文も答えも鍵も書きません。設定ファイルにも鍵は入りません。", { spacing: { before: mm(3), after: mm(2.6) } }),
  p("設計の詳しい説明は `docs/architecture.md`、手順の一覧は `docs/usage.md` にあります。"),
];

// ---- 文書 ---------------------------------------------------------------
const doc = new Document({
  creator: "pet-claude", title: "pet-claude マニュアル", description: "Claude Code と一緒に動く Windows のデスクトップペットの使い方",
  styles: {
    default: { document: { run: { font: { ascii: FONT, hAnsi: FONT, eastAsia: FONT, cs: FONT }, size: PT(10.5), color: INK }, paragraph: { spacing: { line: 340 } } } },
    paragraphStyles: [
      // Normal を明示する。無いと日本語の Word が自前の既定（游明朝＝テーマの本文フォント）で補い、docDefaults の指定が負ける（2026-10-28 実測）
      { id: "Normal", name: "Normal", quickFormat: true,
        run: { font: { ascii: FONT, hAnsi: FONT, eastAsia: FONT, cs: FONT }, size: PT(10.5), color: INK, language: { value: "ja-JP", eastAsia: "ja-JP" } },
        paragraph: { spacing: { line: 340 } } },
      { id: "Heading1", name: "Heading 1", basedOn: "Normal", next: "Normal", quickFormat: true,
        run: { size: PT(16), bold: true, color: INK, font: { ascii: FONT, hAnsi: FONT, eastAsia: FONT } },
        paragraph: { spacing: { before: 0, after: mm(5) }, outlineLevel: 0, border: { bottom: { style: BorderStyle.SINGLE, size: 12, color: INK, space: 4 } } } },
      { id: "Heading2", name: "Heading 2", basedOn: "Normal", next: "Normal", quickFormat: true,
        run: { size: PT(11.5), bold: true, color: INK, font: { ascii: FONT, hAnsi: FONT, eastAsia: FONT } },
        paragraph: { spacing: { before: mm(6), after: mm(2) }, outlineLevel: 1 } },
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
      new TextRun({ text: "pet-claude マニュアル ", size: PT(8), color: "888888" }),
      new TextRun({ children: [PageNumber.CURRENT], size: PT(8), color: "888888" }),
      new TextRun({ text: " / ", size: PT(8), color: "888888" }),
      new TextRun({ children: [PageNumber.TOTAL_PAGES], size: PT(8), color: "888888" }),
    ] })] }) },
    children: [...cover, ...toc, ...c1, ...c2, ...c3, ...c4, ...c5, ...c6, ...c7, ...c8],
  }],
});

Packer.toBuffer(doc).then((buf) => {
  fs.writeFileSync(OUT, buf);
  console.log("wrote", OUT, buf.length, "bytes");
});
