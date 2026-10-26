// チャットまわりの UI（設定・チャット欄・答えの吹き出し）を、invoke の偽物つきで描いて撮る。
// 実機の窓の大きさ・明暗 2 通り。エンジンは WebView2 と同じ Edge（playwright の channel: "msedge"）。
// 実機の撮影では、入力の無い試験のプロセスは前面を取れず、フォーカスを持つ窓（チャット欄・設定）がすぐ閉じる・
// 裏に回るので撮れない（2026-10-21）。見た目はこれで確かめ、クリックの経路は人の実機確認で見る。
// 使い方: node scripts/verify/render-ui.js <出力フォルダ>（playwright は npm のグローバルにあるものを使う）
const path = require("path");
const pw = require(path.join(process.env.APPDATA, "npm/node_modules/@playwright/cli/node_modules/playwright"));
const ui = path.resolve(__dirname, "../../ui").split(path.sep).join("/");
const out = process.argv[2];

const answer = "たくさん思いつくよ！食べ物なら、焼き芋、栗ご飯、さんま、新米、柿、梨、ぶどう、きのこ料理、月見団子。お出かけなら、紅葉狩り、ハイキング、芋掘り、りんご狩り、温泉。おうちなら、読書、温かいお茶、秋の映画やゲーム。\n\n夜は虫の声を聞くのもいいね。";
const bubbleText = answer.replace(/\s+/g, " ").slice(0, 139) + "…";
const cases = [
  { name: "settings-sub", page: "settings.html", size: [440, 600],
    state: { settings_get: { scale: 0.55, tts: true, wake: false, walk: false, pet: "a", pets: [{ id: "a", name: "まめドラゴン", wakeWord: "まめちゃん" }, { id: "placeholder", name: "仮キャラ", wakeWord: null }], chatAuth: "subscription", hasApiKey: false }, autostart_get: { state: "off", detail: "" } } },
  { name: "settings-placeholder", page: "settings.html", size: [440, 600],
    state: { settings_get: { scale: 0.55, tts: true, wake: false, walk: false, pet: "placeholder", pets: [{ id: "a", name: "まめドラゴン", wakeWord: "まめちゃん" }, { id: "placeholder", name: "仮キャラ", wakeWord: null }], chatAuth: "subscription", hasApiKey: false } } },
  { name: "settings-key-empty", page: "settings.html", size: [440, 600],
    state: { settings_get: { scale: 0.55, tts: true, wake: false, walk: false, pet: "a", pets: [{ id: "a", name: "まめドラゴン", wakeWord: "まめちゃん" }, { id: "placeholder", name: "仮キャラ", wakeWord: null }], chatAuth: "api_key", hasApiKey: false } } },
  { name: "settings-key-saved", page: "settings.html", size: [440, 600],
    state: { settings_get: { scale: 0.55, tts: true, wake: false, walk: false, pet: "a", pets: [{ id: "a", name: "まめドラゴン", wakeWord: "まめちゃん" }, { id: "placeholder", name: "仮キャラ", wakeWord: null }], chatAuth: "api_key", hasApiKey: true } } },
  // 自動起動（Phase 4）: 登録済み・別の場所の exe・失敗
  ...[["on", ""], ["other", "C:\Users\k\dev\pet-claude\pet-claude-main\target\debug\pet-app.exe"], ["error", "C:\Users\k\.claude\scripts\run-hidden.vbs が無い"]].map(([st, detail]) => (
    { name: "settings-autostart-" + st, page: "settings.html", size: [440, 600],
      state: { settings_get: { scale: 0.55, tts: true, wake: false, walk: false, pet: "a", pets: [{ id: "a", name: "まめドラゴン", wakeWord: "まめちゃん" }], chatAuth: "subscription", hasApiKey: false },
        autostart_get: { state: st, detail } } })),
  { name: "chat-answer", page: "chat.html", size: [384, 48 + 8 + 188 + 24], state: { chat_state: { answer, error: false, busy: false } } },
  { name: "chat-busy", page: "chat.html", size: [384, 48 + 8 + 188 + 24], state: { chat_state: { answer: "こんにちは！今日はどうしたの？", error: false, busy: true } } },
  { name: "chat-first-wait", page: "chat.html", size: [384, 48 + 8 + 188 + 24], state: { chat_state: { answer: null, error: false, busy: true } } },
  { name: "chat-error", page: "chat.html", size: [384, 48 + 8 + 188 + 24], state: { chat_state: { answer: "時間切れ（120 秒）", error: true, busy: false } } },
  // 音声入力で聞き取った文を K の発言として出す（Phase 3 後半）
  { name: "chat-heard-busy", page: "chat.html", size: [384, 48 + 8 + 188 + 24],
    state: { chat_state: { answer: null, error: false, busy: true, heard: "まめちゃん、明日の天気どうかな" } } },
  { name: "chat-heard-answer", page: "chat.html", size: [384, 48 + 8 + 188 + 24],
    state: { chat_state: { answer: "明日は晴れそうだよ。朝は少し冷えるから上着があるといいね。", error: false, busy: false, heard: "まめちゃん、明日の天気どうかな" } } },
  // 打って送る経路を画面のスクリプトで通す（2026-10-23 評価係の指摘: 送信は Rust 側から呼ぶ試験しか無く、
  // chat.html の SyntaxError を誰も踏まなかった）。打って送信を押し、invoke("chat_submit") が届くかを見る
  { name: "chat-submit", page: "chat.html", size: [384, 48 + 24],
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
      // 島がはみ出していないか（スクロールが出ていないか）
      const over = await page.evaluate(() => [...document.querySelectorAll(".island, main")].map((e) => e.scrollHeight - e.clientHeight));
      let acted = "";
      if (c.act) {
        const r = await c.act(page);
        if (!r.ok) failed++;
        acted = ` act=${r.ok ? "PASS" : "FAIL"} ${r.detail}`;
      }
      if (errors.length) failed++;
      await page.screenshot({ path: `${out}/${scheme}-${c.name}.png` });
      console.log(scheme, c.name, "overflow", JSON.stringify(over), errors.length ? `PAGEERROR ${errors.join(" | ")}` : "", acted);
      await ctx.close();
    }
  }
  await browser.close();
  console.log(failed ? `FAIL ${failed}` : "ALL OK");
  process.exit(failed ? 1 : 0);
})();
