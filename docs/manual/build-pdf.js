// manual.html を Edge（playwright の channel: "msedge"）で A4 の PDF にする。日本語フォントは Edge が埋め込む。
// 使い方: node docs/manual/build-pdf.js  → docs/manual/pet-claude-manual.pdf
//   playwright は npm のグローバル（@playwright/cli 同梱）を使う（scripts/verify/render-ui.js と同じ）。
//   作った後は python docs/manual/check-pdf.py で、ページ数・フォントの埋め込み・リンク切れを確かめる。
const path = require("path");
const pw = require(path.join(process.env.APPDATA, "npm/node_modules/@playwright/cli/node_modules/playwright"));
const html = path.resolve(__dirname, "manual.html");
const out = path.resolve(__dirname, "pet-claude-manual.pdf");

(async () => {
  const browser = await pw.chromium.launch({ channel: "msedge" });
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e.message || e)));
  page.on("requestfailed", (r) => errors.push("load failed: " + r.url()));
  await page.goto("file:///" + html.split(path.sep).join("/"), { waitUntil: "load" });
  await page.emulateMedia({ media: "print" });
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(300);
  await page.pdf({
    path: out,
    format: "A4",
    printBackground: true,
    tagged: true,
    outline: true,
    preferCSSPageSize: true,
    displayHeaderFooter: true,
    headerTemplate: "<span></span>",
    footerTemplate: '<div style="width:100%;text-align:center;font-size:8px;color:#888;font-family:sans-serif;">'
      + 'pet-claude マニュアル <span class="pageNumber"></span> / <span class="totalPages"></span></div>',
  });
  await browser.close();
  if (errors.length) {
    console.error("errors:", errors.join(" | "));
    process.exit(1);
  }
  console.log("wrote", out);
})();
