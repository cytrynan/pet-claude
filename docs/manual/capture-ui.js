// マニュアル用に、設定画面とチャット欄を Edge で描いて撮る（scripts/verify/render-ui.js と同じ仕組み: invoke の偽物）。
// 実機では、入力の無い試験のプロセスから前面を取れず、フォーカスを持つ窓（設定・チャット欄）を撮れないため。
// 使い方: node docs/manual/capture-ui.js <出力フォルダ> [設定画面の HTML のフォルダ]
//   playwright は npm のグローバル（@playwright/cli 同梱）を使う。2 つ目の引数を省くとリポジトリの ui/ を使う。
//   2026-10-28 の撮影では、設定の欄を外した後の形（fix/remove-wake-setting 側の ui/）を 2 つ目の引数で渡した。
const path = require("path");
const pw = require(path.join(process.env.APPDATA, "npm/node_modules/@playwright/cli/node_modules/playwright"));
const repoUi = path.resolve(__dirname, "../../ui");
const out = process.argv[2];
const settingsUi = process.argv[3] ? path.resolve(process.argv[3]) : repoUi;
if (!out) { console.error("usage: node capture-ui.js <out-dir> [settings-ui-dir]"); process.exit(2); }
const fileUrl = (dir, page) => "file:///" + dir.split(path.sep).join("/") + "/" + page;

// 設定画面に出す値（K の環境の既定に近い形。個人のパスは出さない）
const pets = [
  { id: "mame-dragon", name: "まめドラゴン", wakeWord: "まめちゃん" },
  { id: "rinrin", name: "りんりん", wakeWord: "りんちゃん" },
  { id: "tora", name: "とらちゃん", wakeWord: "とらちゃん" },
  { id: "kuroshiba", name: "しばちゃん", wakeWord: "しばちゃん" },
  { id: "komugi", name: "こむぎちゃん", wakeWord: "こむぎちゃん" },
  { id: "goma", name: "ごまちゃん", wakeWord: "ごまちゃん" },
];
const settings = {
  settings_get: { scale: 0.55, tts: true, wake: false, walk: false, ask: true, pet: "mame-dragon", pets,
    chatAuth: "subscription", hasApiKey: false, chatModel: "fable", chatEffort: "high", chatMode: "auto",
    chatDir: null, chatDirDefault: "C:\\Users\\<ユーザー名>" },
  autostart_get: { state: "on", detail: "" },
  voice_status: { tts: "使えます", stt: "使えます" },
};
const answer = "今日の空は秋晴れだね。散歩なら夕方がいいよ。";
const chatOpts = { model: "fable", effort: "high", mode: "auto" };
const cases = [
  { name: "settings", dir: settingsUi, page: "settings.html", size: [440, 1180], state: settings },
  { name: "chat-empty", dir: repoUi, page: "chat.html", size: [384, 80 + 24],
    state: { chat_state: { answer: null, error: false, busy: false, options: chatOpts } } },
  { name: "chat-answer", dir: repoUi, page: "chat.html", size: [384, 80 + 8 + 264 + 24],
    state: { chat_state: { answer, error: false, busy: false, options: chatOpts } } },
  { name: "chat-menu-mode", dir: repoUi, page: "chat.html", size: [384, 80 + 8 + 290 + 24],
    state: { chat_state: { answer: null, error: false, busy: false, options: chatOpts } },
    act: (page) => page.click("#mode-btn") },
  { name: "chat-menu-model", dir: repoUi, page: "chat.html", size: [384, 80 + 8 + 220 + 24],
    state: { chat_state: { answer: null, error: false, busy: false, options: chatOpts } },
    act: (page) => page.click("#model-btn") },
];

(async () => {
  const browser = await pw.chromium.launch({ channel: "msedge" });
  for (const c of cases) {
    const ctx = await browser.newContext({ viewport: { width: c.size[0], height: c.size[1] }, colorScheme: "dark", deviceScaleFactor: 2 });
    const page = await ctx.newPage();
    const errors = [];
    page.on("pageerror", (e) => errors.push(String(e.message || e)));
    await page.addInitScript((st) => {
      window.__TAURI__ = { core: { invoke: async (cmd) => (cmd in st ? st[cmd] : null) } };
    }, c.state);
    await page.goto(fileUrl(c.dir, c.page));
    await page.waitForTimeout(400);
    if (c.act) { await c.act(page); await page.waitForTimeout(200); }
    const file = `${out}/${c.name}.png`;
    await page.screenshot({ path: file, omitBackground: c.page !== "settings.html" });
    console.log(c.name, errors.length ? `PAGEERROR ${errors.join(" | ")}` : "ok", file);
    await ctx.close();
  }
  await browser.close();
})();
