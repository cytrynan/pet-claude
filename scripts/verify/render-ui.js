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
    state: { settings_get: { scale: 0.55, tts: true, wake: false, walk: false, pet: "a", pets: [{ id: "a", name: "まめドラゴン", wakeWord: "まめちゃん" }, { id: "placeholder", name: "仮キャラ", wakeWord: null }], chatAuth: "subscription", hasApiKey: false } } },
  { name: "settings-placeholder", page: "settings.html", size: [440, 600],
    state: { settings_get: { scale: 0.55, tts: true, wake: false, walk: false, pet: "placeholder", pets: [{ id: "a", name: "まめドラゴン", wakeWord: "まめちゃん" }, { id: "placeholder", name: "仮キャラ", wakeWord: null }], chatAuth: "subscription", hasApiKey: false } } },
  { name: "settings-key-empty", page: "settings.html", size: [440, 600],
    state: { settings_get: { scale: 0.55, tts: true, wake: false, walk: false, pet: "a", pets: [{ id: "a", name: "まめドラゴン", wakeWord: "まめちゃん" }, { id: "placeholder", name: "仮キャラ", wakeWord: null }], chatAuth: "api_key", hasApiKey: false } } },
  { name: "settings-key-saved", page: "settings.html", size: [440, 600],
    state: { settings_get: { scale: 0.55, tts: true, wake: false, walk: false, pet: "a", pets: [{ id: "a", name: "まめドラゴン", wakeWord: "まめちゃん" }, { id: "placeholder", name: "仮キャラ", wakeWord: null }], chatAuth: "api_key", hasApiKey: true } } },
  { name: "chat-answer", page: "chat.html", size: [384, 48 + 8 + 188 + 24], state: { chat_state: { answer, error: false, busy: false } } },
  { name: "chat-busy", page: "chat.html", size: [384, 48 + 8 + 188 + 24], state: { chat_state: { answer: "こんにちは！今日はどうしたの？", error: false, busy: true } } },
  { name: "chat-first-wait", page: "chat.html", size: [384, 48 + 8 + 188 + 24], state: { chat_state: { answer: null, error: false, busy: true } } },
  { name: "chat-error", page: "chat.html", size: [384, 48 + 8 + 188 + 24], state: { chat_state: { answer: "時間切れ（120 秒）", error: true, busy: false } } },
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
];

(async () => {
  const browser = await pw.chromium.launch({ channel: "msedge" });
  for (const scheme of ["dark", "light"]) {
    for (const c of cases) {
      const ctx = await browser.newContext({ viewport: { width: c.size[0], height: c.size[1] }, colorScheme: scheme, deviceScaleFactor: 2 });
      const page = await ctx.newPage();
      await page.addInitScript((st) => {
        window.__TAURI__ = { core: { invoke: async (cmd) => (cmd in st ? st[cmd] : null) } };
      }, c.state);
      await page.goto("file:///" + ui + "/" + c.page);
      await page.waitForTimeout(300);
      // 島がはみ出していないか（スクロールが出ていないか）
      const over = await page.evaluate(() => [...document.querySelectorAll(".island, main")].map((e) => e.scrollHeight - e.clientHeight));
      // ボタンの行が横にはみ出していないか（文言が長くなる承認のカード）。右寄せの行は左へはみ出し、
      // scrollWidth には出ないので、子の左右の端を行の箱と比べる（正ならはみ出した px）
      const wide = await page.evaluate(() => [...document.querySelectorAll(".actions")].map((e) => {
        const box = e.getBoundingClientRect();
        const kids = [...e.children].map((c) => c.getBoundingClientRect());
        if (!kids.length) return 0;
        return Math.ceil(Math.max(box.left - Math.min(...kids.map((k) => k.left)), Math.max(...kids.map((k) => k.right)) - box.right, 0));
      }));
      await page.screenshot({ path: `${out}/${scheme}-${c.name}.png` });
      console.log(scheme, c.name, "overflow", JSON.stringify(over), "wide", JSON.stringify(wide));
      await ctx.close();
    }
  }
  await browser.close();
})();
