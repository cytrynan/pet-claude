// チャットまわりの UI（設定・チャット欄・答えの吹き出し）を、invoke の偽物つきで描いて撮る。
// 実機の窓の大きさ・明暗 2 通り。エンジンは WebView2 と同じ Edge（playwright の channel: "msedge"）。
// 実機の撮影では、入力の無い試験のプロセスは前面を取れず、フォーカスを持つ窓（チャット欄・設定）がすぐ閉じる・
// 裏に回るので撮れない（2026-10-21）。見た目はこれで確かめ、クリックの経路は人の実機確認で見る。
// 使い方: node scripts/verify/render-ui.js <出力フォルダ>（playwright は npm のグローバルにあるものを使う）
const path = require("path");
const { execFileSync } = require("child_process");
const os = require("os");
const pw = require(path.join(process.env.APPDATA, "npm/node_modules/@playwright/cli/node_modules/playwright"));
const ui = path.resolve(__dirname, "../../ui").split(path.sep).join("/");
const out = process.argv[2];

// チャット欄の答えの欄の高さ（ui.rs の CHAT_ANSWER_HEIGHT・chat.html の .answer と同じ。一番高いメニューに合わせた値）
const ANSWER_H = 264;
const answer = "たくさん思いつくよ！食べ物なら、焼き芋、栗ご飯、さんま、新米、柿、梨、ぶどう、きのこ料理、月見団子。お出かけなら、紅葉狩り、ハイキング、芋掘り、りんご狩り、温泉。おうちなら、読書、温かいお茶、秋の映画やゲーム。\n\n夜は虫の声を聞くのもいいね。";
const bubbleText = answer.replace(/\s+/g, " ").slice(0, 139) + "…";
// 固定文言の吹き出し: 窓の大きさはアプリと同じ関数（pet_core::bubble::status_island）から取る（2026-10-27。
// 手で写すと式がずれる）。長い文は省略せず、幅は 320 まで・それ以上は折り返して高さで取る
const statusTexts = {
  "status-short": "できたよ",
  "status-k": "マイクを使えないよ（ミュートか Windows の設定）",
  "status-60": "答えを読み上げられなかったよ。AivisSpeech が起動していないか、音声の出力先が見つからないみたい。設定を見てね。",
  "status-120": "いま話した文を送れなかったよ。Claude Code が見つからないか、ログインが切れているかもしれない。設定の「いまの接続」で確かめてから、もう一度バーの音声ボタンを押して話してみてね。それでもだめなら再起動してみて。",
  "status-200": "あ".repeat(200),
  "status-newline": "できたよ\nほかに 2 件まってるよ\n\n設定も見てね",
  "status-url": "詳しくは https://github.com/cytrynan/pet-claude/blob/main/docs/architecture.md#63-音声入力 を見てね",
  "status-latin": "WINDOWS MICROPHONE is muted. Check the Windows sound settings and the mute key on the keyboard.",
};
const cargo = process.env.CARGO || path.join(os.homedir(), ".cargo", "bin", "cargo");
const statusSizes = JSON.parse(execFileSync(cargo, ["run", "-q", "-p", "pet-core", "--example", "bubble_size", "--", JSON.stringify(Object.values(statusTexts))],
  { cwd: path.resolve(__dirname, "../.."), encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] }));
const statusCases = Object.entries(statusTexts).map(([name, text], i) => ({
  name, page: "bubble.html", size: [statusSizes[i].w + 24, statusSizes[i].h + 24],
  state: { bubble_state: { mode: "status", text, lines: statusSizes[i].lines, seq: 1 } } }));

const cases = [
  ...statusCases,
  { name: "settings-sub", page: "settings.html", size: [440, 960],
    state: { settings_get: { scale: 0.55, tts: true, wake: false, walk: false, pet: "a", pets: [{ id: "a", name: "まめドラゴン", wakeWord: "まめちゃん" }, { id: "placeholder", name: "仮キャラ", wakeWord: null }], chatAuth: "subscription", hasApiKey: false }, autostart_get: { state: "off", detail: "" } } },
  // チャットの既定 4 項目（2026-10-27）: 設定済みの値と長い作業フォルダ
  { name: "settings-chat-defaults", page: "settings.html", size: [440, 1200],
    state: { settings_get: { scale: 0.55, tts: true, wake: false, walk: false, pet: "a", pets: [{ id: "a", name: "まめドラゴン", wakeWord: "まめちゃん" }], chatAuth: "subscription", hasApiKey: false,
      chatModel: "sonnet", chatEffort: "medium", chatMode: "acceptEdits", chatDir: "C:\\Users\\k\\dev\\pet-claude\\pet-claude-main\\very\\long\\path", chatDirDefault: "C:\\Users\\k" } },
    act: async (page) => {
      const v = await page.evaluate(() => [document.getElementById("model").value, document.getElementById("effort").value, document.getElementById("mode").value,
        document.getElementById("chat-dir").textContent, document.getElementById("dir-reset").hidden]);
      const ok = v[0] === "sonnet" && v[1] === "medium" && v[2] === "acceptEdits" && v[3].endsWith("path") && v[4] === false;
      return { ok, detail: JSON.stringify(v) };
    } },
  // 知らない値は既定（Fable・高・自動）で見せ、作業フォルダは既定（ホーム）を出して「既定に戻す」は隠す
  { name: "settings-chat-unknown", page: "settings.html", size: [440, 1200],
    state: { settings_get: { scale: 0.55, tts: true, wake: false, walk: false, pet: "a", pets: [{ id: "a", name: "まめドラゴン", wakeWord: "まめちゃん" }], chatAuth: "subscription", hasApiKey: false,
      chatModel: "gpt", chatEffort: "ultra", chatMode: "root", chatDir: null, chatDirDefault: "C:\\Users\\k" } },
    act: async (page) => {
      const v = await page.evaluate(() => [document.getElementById("model").value, document.getElementById("effort").value, document.getElementById("mode").value,
        document.getElementById("chat-dir").textContent, document.getElementById("dir-reset").hidden]);
      const ok = v[0] === "fable" && v[1] === "high" && v[2] === "auto" && v[3] === "C:\\Users\\k" && v[4] === true;
      return { ok, detail: JSON.stringify(v) };
    } },
  { name: "settings-placeholder", page: "settings.html", size: [440, 960],
    state: { settings_get: { scale: 0.55, tts: true, wake: false, walk: false, pet: "placeholder", pets: [{ id: "a", name: "まめドラゴン", wakeWord: "まめちゃん" }, { id: "placeholder", name: "仮キャラ", wakeWord: null }], chatAuth: "subscription", hasApiKey: false } } },
  { name: "settings-key-empty", page: "settings.html", size: [440, 960],
    state: { settings_get: { scale: 0.55, tts: true, wake: false, walk: false, pet: "a", pets: [{ id: "a", name: "まめドラゴン", wakeWord: "まめちゃん" }, { id: "placeholder", name: "仮キャラ", wakeWord: null }], chatAuth: "api_key", hasApiKey: false } } },
  { name: "settings-key-saved", page: "settings.html", size: [440, 960],
    state: { settings_get: { scale: 0.55, tts: true, wake: false, walk: false, pet: "a", pets: [{ id: "a", name: "まめドラゴン", wakeWord: "まめちゃん" }, { id: "placeholder", name: "仮キャラ", wakeWord: null }], chatAuth: "api_key", hasApiKey: true } } },
  // 自動起動（Phase 4）: 登録済み・別の場所の exe・失敗
  ...[["on", ""], ["other", "C:\Users\k\dev\pet-claude\pet-claude-main\target\debug\pet-app.exe"], ["error", "C:\Users\k\.claude\scripts\run-hidden.vbs が無い"]].map(([st, detail]) => (
    { name: "settings-autostart-" + st, page: "settings.html", size: [440, 960],
      state: { settings_get: { scale: 0.55, tts: true, wake: false, walk: false, pet: "a", pets: [{ id: "a", name: "まめドラゴン", wakeWord: "まめちゃん" }], chatAuth: "subscription", hasApiKey: false },
        autostart_get: { state: st, detail } } })),
  // チャット欄の下の行（モード・モデル＋エフォート）とメニュー（2026-10-27）。メニューは入力の島の上に伸びる。
  // 窓の高さはメニューの分（ui.rs の set_chat_menu_height に伝える offsetHeight）を見込んで取る
  { name: "chat-opts-nondefault", page: "chat.html", size: [384, 80 + 24],
    state: { chat_state: { answer: null, error: false, busy: false, options: { model: "haiku", effort: "max", mode: "bypassPermissions" } } } },
  ...[["mode", "#mode-btn", 5, 290], ["model", "#model-btn", 4, 220]].map(([kind, btn, rows, h]) => (
    { name: "chat-menu-" + kind, page: "chat.html", size: [384, 80 + 8 + h + 24],
      state: { chat_state: { answer: null, error: false, busy: false, options: { model: "fable", effort: "high", mode: "auto" } }, chat_options_set: { model: "opus", effort: "high", mode: "auto" } },
      act: async (page) => {
        await page.click(btn);
        await page.waitForTimeout(100);
        const calls = await page.evaluate(() => window.__calls);
        const opened = calls.find((c) => c.cmd === "chat_menu");
        const n = await page.locator('#menu-items [role="menuitemradio"]').count();
        const checked = await page.locator('#menu-items [aria-checked="true"]').count();
        const menuH = await page.evaluate(() => document.getElementById("menu").offsetHeight);
        const ok = !!opened && opened.args.height > 0 && opened.args.height <= h && n === rows && checked === 1 && menuH === opened.args.height;
        return { ok, detail: `chat_menu=${JSON.stringify(opened && opened.args)} rows=${n} checked=${checked} menuH=${menuH}` };
      } })),
  // 答えが出ている時にメニューを開く（2026-10-28 K 報告「自動やモデルを押すと会話が一瞬消える」）。
  // 窓（viewport）は答えありの高さのまま。開いた瞬間（クリック直後）と開いた後（300ms）を撮り、答えの欄が
  // 隠されず・位置も文も変わらないこと、Rust に伝える高さが答えの欄に収まる（＝窓を動かさない）ことを見る
  ...[["mode", "#mode-btn"], ["model", "#model-btn"]].map(([kind, btn]) => (
    { name: "chat-menu-" + kind + "-answer", page: "chat.html", size: [384, 80 + 8 + ANSWER_H + 24],
      state: { chat_state: { answer, error: false, busy: false, options: { model: "fable", effort: "high", mode: "auto" } } },
      act: async (page) => {
        const look = () => page.evaluate(() => {
          const a = document.getElementById("answer");
          const r = a.getBoundingClientRect();
          return { hidden: a.hidden, rect: [r.left, r.top, r.width, r.height].join(","), text: document.getElementById("answer-text").textContent };
        });
        const scheme = await page.evaluate(() => matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
        const before = await look();
        await page.click(btn);
        await page.screenshot({ path: `${out}/${scheme}-chat-menu-${kind}-answer-instant.png` });
        const instant = await look();
        await page.waitForTimeout(300);
        await page.screenshot({ path: `${out}/${scheme}-chat-menu-${kind}-answer-after.png` });
        const after = await look();
        const calls = await page.evaluate(() => window.__calls);
        const opened = calls.find((c) => c.cmd === "chat_menu");
        const menuH = await page.evaluate(() => document.getElementById("menu").offsetHeight);
        const same = (x) => !x.hidden && x.rect === before.rect && x.text === before.text && x.text.length > 0;
        const ok = same(instant) && same(after) && !!opened && opened.args.height > 0 && opened.args.height <= ANSWER_H && menuH <= ANSWER_H;
        return { ok, detail: `answer=${before.rect} instant=${instant.rect} after=${after.rect} chat_menu=${JSON.stringify(opened && opened.args)} menuH=${menuH}` };
      } })),
  // エフォートの一覧（モデルのメニューの「エフォート ›」から）。選ぶと chat_options_set が届き、閉じて（高さ 0）表示が変わる
  { name: "chat-menu-effort", page: "chat.html", size: [384, 80 + 8 + 220 + 24],
    state: { chat_state: { answer: null, error: false, busy: false, options: { model: "fable", effort: "high", mode: "auto" } }, chat_options_set: { model: "fable", effort: "max", mode: "auto" } },
    act: async (page) => {
      await page.click("#model-btn");
      await page.click('#menu-items [aria-haspopup="menu"]');
      await page.waitForTimeout(100);
      const n = await page.locator('#menu-items [role="menuitemradio"]').count();
      const title = await page.textContent("#menu-title");
      await page.screenshot({ path: `${out}/${await page.evaluate(() => matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light")}-chat-menu-effort-open.png` });
      await page.click('#menu-items [data-i="5"]');
      await page.waitForTimeout(100);
      const calls = await page.evaluate(() => window.__calls);
      const set = calls.find((c) => c.cmd === "chat_options_set");
      const closed = calls.filter((c) => c.cmd === "chat_menu").at(-1);
      const label = await page.textContent("#model-btn");
      const hidden = await page.evaluate(() => document.getElementById("menu").hidden);
      const ok = n === 5 && title === "エフォート" && !!set && set.args.effort === "max" && !!closed && closed.args.height === 0 && hidden && label === "Fable 5.1最大";
      return { ok, detail: `rows=${n} title=${title} set=${JSON.stringify(set && set.args)} closed=${JSON.stringify(closed && closed.args)} label=${JSON.stringify(label)}` };
    } },
  { name: "chat-answer", page: "chat.html", size: [384, 80 + 8 + ANSWER_H + 24], state: { chat_state: { answer, error: false, busy: false } } },
  { name: "chat-busy", page: "chat.html", size: [384, 80 + 8 + ANSWER_H + 24], state: { chat_state: { answer: "こんにちは！今日はどうしたの？", error: false, busy: true } } },
  { name: "chat-first-wait", page: "chat.html", size: [384, 80 + 8 + ANSWER_H + 24], state: { chat_state: { answer: null, error: false, busy: true } } },
  { name: "chat-error", page: "chat.html", size: [384, 80 + 8 + ANSWER_H + 24], state: { chat_state: { answer: "時間切れ（120 秒）", error: true, busy: false } } },
  // 音声入力で聞き取った文を K の発言として出す（Phase 3 後半）
  { name: "chat-heard-busy", page: "chat.html", size: [384, 80 + 8 + ANSWER_H + 24],
    state: { chat_state: { answer: null, error: false, busy: true, heard: "まめちゃん、明日の天気どうかな" } } },
  { name: "chat-heard-answer", page: "chat.html", size: [384, 80 + 8 + ANSWER_H + 24],
    state: { chat_state: { answer: "明日は晴れそうだよ。朝は少し冷えるから上着があるといいね。", error: false, busy: false, heard: "まめちゃん、明日の天気どうかな" } } },
  // 打って送る経路を画面のスクリプトで通す（2026-10-23 評価係の指摘: 送信は Rust 側から呼ぶ試験しか無く、
  // chat.html の SyntaxError を誰も踏まなかった）。打って送信を押し、invoke("chat_submit") が届くかを見る
  { name: "chat-submit", page: "chat.html", size: [384, 80 + 24],
    state: { chat_state: { answer: null, error: false, busy: false }, chat_submit: "考え中…" },
    act: async (page) => {
      await page.fill("#text", "明日の天気は？");
      await page.click("#send");
      await page.waitForTimeout(200);
      const calls = await page.evaluate(() => window.__calls);
      const sent = calls.some((c) => c.cmd === "chat_submit" && c.args && c.args.text === "明日の天気は？");
      const left = await page.inputValue("#text");
      return { ok: sent && left === "", detail: `calls=${JSON.stringify(calls.map((c) => c.cmd))} input="${left}"` };
    } },
  { name: "bubble-answer", page: "bubble.html", size: [324, 44 + 7 * 20 + 34 + 24],
    state: { bubble_state: { mode: "answer", text: bubbleText, more: true, error: false, lines: 7 } } },
  { name: "bubble-short", page: "bubble.html", size: [324, 44 + 24],
    state: { bubble_state: { mode: "answer", text: "こんにちは！", more: false, error: false, lines: 1 } } },
  // 承認のカード: 「〜で」の文言は来た先（ターミナル・Claude アプリ・分からない）で変わる。いちばん長い組み合わせ（ほかに N 件つき）も撮る
  { name: "perm-terminal", page: "bubble.html", size: [344, 148 + 24],
    state: { bubble_state: { mode: "permission", id: "p1", tool: "Bash", summary: "cargo test -p pet-core", canAllow: true, lines: 1, more: 0, askLabel: "ターミナルで", note: "全文が見えないので、許可はターミナルで" } } },
  { name: "perm-app", page: "bubble.html", size: [344, 148 + 24],
    state: { bubble_state: { mode: "permission", id: "p1", tool: "Bash", summary: "cargo test -p pet-core", canAllow: true, lines: 1, more: 0, askLabel: "Claude アプリで", note: "全文が見えないので、許可は Claude アプリで" } } },
  { name: "perm-app-more", page: "bubble.html", size: [344, 148 + 24],
    state: { bubble_state: { mode: "permission", id: "p1", tool: "Bash", summary: "cargo test -p pet-core", canAllow: true, lines: 1, more: 12, askLabel: "Claude アプリで", note: "全文が見えないので、許可は Claude アプリで" } } },
  { name: "perm-unknown-more", page: "bubble.html", size: [344, 148 + 24],
    state: { bubble_state: { mode: "permission", id: "p1", tool: "Bash", summary: "cargo test -p pet-core", canAllow: true, lines: 1, more: 12, askLabel: "Claude Code で", note: "全文が見えないので、許可は Claude Code で" } } },
  { name: "perm-app-note-more", page: "bubble.html", size: [344, 172 + 24],
    state: { bubble_state: { mode: "permission", id: "p1", tool: "Bash", summary: "cargo test -p pet-core", canAllow: false, lines: 1, more: 12, askLabel: "Claude アプリで", note: "全文が見えないので、許可は Claude アプリで" } } },
  // 質問のカード（AskUserQuestion）。高さは ui.rs と同じ式: 92 ＋ 質問の行数×18 ＋ 選択肢（ラベルだけ 30・説明つき 44・回答を書く 30・間 4×選択肢の数）
  ...[
    // 1 問・説明つき 3 つ（司令塔が実測した入力）・ターミナル
    { name: "ask-single", height: 92 + 18 + (44 * 3 + 30 + 4 * 3),
      state: { mode: "ask", id: "q1", key: "q1:0", header: "好きな色", question: "どの色が好きですか？", multi: false, more: 0, askLabel: "ターミナルで",
        options: [{ label: "Red", description: "赤", picked: false }, { label: "Green", description: "緑", picked: false }, { label: "Blue", description: "青", picked: false }] } },
    // 複数選択・説明なし 4 つ・2 つ選んである・ほかに 12 件・Claude アプリで（ボタン行が最長）・質問 2 行
    { name: "ask-multi-more", height: 92 + 36 + (30 * 4 + 30 + 4 * 4),
      state: { mode: "ask", id: "q2", key: "q2:0", header: "形 · 1/3", question: "どの形を使いますか？ 複数選んでもかまいません。あとから変えられます", multi: true, more: 12, askLabel: "Claude アプリで",
        options: [{ label: "丸", description: "", picked: true }, { label: "四角", description: "", picked: false }, { label: "三角", description: "", picked: true }, { label: "星", description: "", picked: false }] } },
    // 3 行の質問・長い説明（1 行に省略）4 つ・2/3
    { name: "ask-long", height: 92 + 54 + (44 * 4 + 30 + 4 * 4),
      state: { mode: "ask", id: "q3", key: "q3:1", header: "接続 · 2/3", question: "チャットの接続はどれにしますか？サブスクは Claude Code にログインしているアカウントで答え、API キーは使った分だけ課金されます。", multi: false, more: 1, askLabel: "Claude Code で",
        options: [
          { label: "サブスク", description: "Claude Code にログインしているアカウントで答える。追加の課金は無い", picked: false },
          { label: "API キー", description: "使った分だけ課金される。鍵は Windows の資格情報マネージャーに保存する", picked: false },
          { label: "あとで決める", description: "いまは何も変えない", picked: false },
          { label: "両方試す", description: "サブスクで試してから API キーに切り替える", picked: false }] } },
  ].map((c) => ({ name: c.name, page: "bubble.html", size: [344, c.height + 24], state: { bubble_state: { ...c.state, height: c.height } } })),
  // 選択肢を押す経路（出たばかりの 600ms は押せない → 700ms 待ってから押す）: ask_action の pick が番号で届く
  { name: "ask-pick", page: "bubble.html", size: [344, 92 + 18 + (30 * 2 + 30 + 4 * 2) + 24],
    state: { bubble_state: { mode: "ask", id: "q4", key: "q4:0", header: "質問", question: "どっち？", multi: false, more: 0, askLabel: "ターミナルで", height: 92 + 18 + (30 * 2 + 30 + 4 * 2),
      options: [{ label: "A", description: "", picked: false }, { label: "B", description: "", picked: false }] }, ask_action: "ok" },
    act: async (page) => {
      await page.waitForTimeout(700);
      await page.click('[data-i="1"]');
      await page.click('[data-a="text"]');
      await page.waitForTimeout(100);
      const calls = (await page.evaluate(() => window.__calls)).filter((c) => c.cmd === "ask_action");
      const ok = calls.length === 2 && calls[0].args.action === "pick" && calls[0].args.value === "1" && calls[0].args.id === "q4" && calls[1].args.action === "text";
      return { ok, detail: JSON.stringify(calls.map((c) => c.args)) };
    } },
  // チャット欄が質問への回答の入力になっている: 質問文を出し、送信は ask_action の answer に向く
  { name: "chat-ask", page: "chat.html", size: [384, 80 + 8 + ANSWER_H + 24],
    state: { chat_state: { answer: null, error: false, busy: false, ask: { id: "q1", question: "どの色が好きですか？" } }, ask_action: "ok" },
    act: async (page) => {
      await page.fill("#text", "むらさき");
      await page.click("#send");
      await page.waitForTimeout(200);
      const calls = await page.evaluate(() => window.__calls);
      const sent = calls.some((c) => c.cmd === "ask_action" && c.args && c.args.action === "answer" && c.args.value === "むらさき" && c.args.id === "q1");
      const noChat = !calls.some((c) => c.cmd === "chat_submit");
      const left = await page.inputValue("#text");
      const title = await page.textContent("#answer-title");
      return { ok: sent && noChat && left === "" && title === "質問", detail: `calls=${JSON.stringify(calls.map((c) => c.cmd))} input="${left}" title=${title}` };
    } },
];

let failed = 0;
(async () => {
  const browser = await pw.chromium.launch({ channel: "msedge" });
  for (const scheme of ["dark", "light"]) {
    for (const c of cases) {
      const ctx = await browser.newContext({ viewport: { width: c.size[0], height: c.size[1] }, colorScheme: scheme, deviceScaleFactor: 2 });
      const page = await ctx.newPage();
      // 画面のスクリプトの例外（構文エラーを含む）は失敗にする
      const errors = [];
      page.on("pageerror", (e) => errors.push(String(e.message || e)));
      await page.addInitScript((st) => {
        window.__calls = [];
        window.__TAURI__ = { core: { invoke: async (cmd, args) => { window.__calls.push({ cmd, args }); return cmd in st ? st[cmd] : null; } } };
      }, c.state);
      await page.goto("file:///" + ui + "/" + c.page);
      await page.waitForTimeout(300);
      // 島がはみ出していないか（スクロールが出ていないか）。縦と横（2026-10-27 固定文言は横にはみ出していた）。0 でなければ失敗
      const over = await page.evaluate(() => [...document.querySelectorAll(".island, main")].map((e) => Math.max(e.scrollHeight - e.clientHeight, e.scrollWidth - e.clientWidth)));
      if (over.some((o) => o > 0)) failed++;
      // ボタンの行が横にはみ出していないか（文言が長くなる承認のカード）。右寄せの行は左へはみ出し、
      // scrollWidth には出ないので、子の左右の端を行の箱と比べる（正ならはみ出した px。0 でなければ失敗）
      const wide = await page.evaluate(() => [...document.querySelectorAll(".actions")].map((e) => {
        const box = e.getBoundingClientRect();
        const kids = [...e.children].map((c) => c.getBoundingClientRect());
        if (!kids.length) return 0;
        return Math.ceil(Math.max(box.left - Math.min(...kids.map((k) => k.left)), Math.max(...kids.map((k) => k.right)) - box.right, 0));
      }));
      if (wide.some((w) => w > 0)) failed++;
      let acted = "";
      if (c.act) {
        const r = await c.act(page);
        if (!r.ok) failed++;
        acted = ` act=${r.ok ? "PASS" : "FAIL"} ${r.detail}`;
      }
      if (errors.length) failed++;
      await page.screenshot({ path: `${out}/${scheme}-${c.name}.png` });
      console.log(scheme, c.name, "overflow", JSON.stringify(over), "wide", JSON.stringify(wide), errors.length ? `PAGEERROR ${errors.join(" | ")}` : "", acted);
      await ctx.close();
    }
  }
  await browser.close();
  console.log(failed ? `FAIL ${failed}` : "ALL OK");
  process.exit(failed ? 1 : 0);
})();
