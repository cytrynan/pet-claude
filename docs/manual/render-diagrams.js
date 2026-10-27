// diagrams.html の .dgm を Edge（playwright の channel: "msedge"）で 2 倍の PNG にする。
// 使い方: node docs/manual/render-diagrams.js  → docs/manual/images/diagrams/<data-id>.png
//   playwright は npm のグローバル（@playwright/cli 同梱）を使う（scripts/verify/render-ui.js と同じ）。
//   図を直したら これを回してから build-docx.js（初版）か Word の「図の変更」で入れ替える。
const fs = require("fs");
const path = require("path");
const pw = require(path.join(process.env.APPDATA, "npm/node_modules/@playwright/cli/node_modules/playwright"));
const html = path.resolve(__dirname, "diagrams.html");
const outDir = path.resolve(__dirname, "images", "diagrams");

(async () => {
  fs.mkdirSync(outDir, { recursive: true });
  const browser = await pw.chromium.launch({ channel: "msedge" });
  const page = await browser.newPage({ viewport: { width: 820, height: 900 }, deviceScaleFactor: 2 });
  await page.goto("file:///" + html.split(path.sep).join("/"), { waitUntil: "load" });
  await page.evaluate(() => document.fonts.ready);
  // $$eval は playwright の DOM 読み取り API（JS の eval ではない）。図の id を data-id から集めるだけ
  const ids = await page.$$eval(".dgm", (els) => els.map((e) => e.dataset.id));
  for (const id of ids) {
    const file = path.join(outDir, `${id}.png`);
    await page.locator(`.dgm[data-id="${id}"]`).screenshot({ path: file, omitBackground: false });
    const kb = Math.round(fs.statSync(file).size / 1024);
    console.log(`${id}.png ${kb}KB`);
    if (kb > 500) { console.error(`${id}.png が 500KB を超えた`); process.exitCode = 1; }
  }
  await browser.close();
})();
