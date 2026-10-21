---
type: Design
description: pet-claude の UI（クリック時のバー・チャット入力・右クリックメニュー・設定）のデザインの単一の正。Apple 風（HIG＋Liquid Glass）を Windows の透過窓に合わせて写す
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-21T09:10:00+09:00 }
---

# DESIGN.md — pet-claude

## 方向性: Apple 風（HIG＋Liquid Glass）

理由: K のグローバル規範で UI の既定は Apple 風（スキル `apple-design`）。手本は K が渡した ChatGPT デスクトップのペット（ペットの下に浮くカプセルのバー、暗い面、白い単色アイコン）。

ペットの周りの UI は「機能層」だけ。中身（コンテンツ層）は無い。だから全部が**浮いた島**になる。

## Windows で写すときの決まり（重要）

- 透過窓の WebView2 では `backdrop-filter` が**背後のデスクトップをぼかせない**（窓の中しかぼけない）。DWM の Acrylic は矩形か 8px 角にしかならず、カプセルが作れない
- そこで**ガラスは「ほぼ不透明の素材」で写す**: 面の不透明度 0.92、上端のハイライトと 0.5px の縁で厚みを出す。ぼかしの代わりに、背景が何でも読める濃さを優先する
- 影は窓の外へ出せないので、窓を面より 12px 大きく取り、その中に `0 8px 24px` の柔らかい影を描く

## トークン

```css
:root {
  --font: -apple-system, BlinkMacSystemFont, "Hiragino Sans", "Hiragino Kaku Gothic ProN",
          "Noto Sans JP", "BIZ UDPGothic", "Yu Gothic UI", Meiryo, sans-serif;
  /* 面（ライト） */
  --surface: rgba(246, 246, 248, .92);
  --surface-hover: rgba(0, 0, 0, .06);
  --surface-press: rgba(0, 0, 0, .10);
  --rim: rgba(0, 0, 0, .10);
  --highlight: rgba(255, 255, 255, .70);
  --label: #1d1d1f;
  --label-2: rgba(60, 60, 67, .60);
  --separator: rgba(60, 60, 67, .18);
  --tint: #0066cc;          /* 白文字で 5.57:1。押せる物だけに使う */
  --shadow: 0 8px 24px rgba(0, 0, 0, .18);
  --ease: cubic-bezier(0.28, 0.11, 0.32, 1);
}
@media (prefers-color-scheme: dark) {
  :root {
    --surface: rgba(38, 38, 40, .92);
    --surface-hover: rgba(255, 255, 255, .08);
    --surface-press: rgba(255, 255, 255, .14);
    --rim: rgba(255, 255, 255, .10);
    --highlight: rgba(255, 255, 255, .10);
    --label: #f5f5f7;
    --label-2: rgba(235, 235, 245, .60);
    --separator: rgba(84, 84, 88, .60);
    --tint: #0a84ff;
    --shadow: 0 8px 24px rgba(0, 0, 0, .45);
  }
}
```

| 項目 | 値 |
|---|---|
| 文字 | 本文 13px（Windows のデスクトップ UI の標準に合わせる。HIG の macOS 本文と同じ）、入力欄 14px、短い UI 文字だけ `palt` |
| 角丸 | バーと入力欄はカプセル（999px）、メニューは 12px、メニューの行は 8px |
| 当たり判定 | アイコンボタン 36×36px 以上（HIG の macOS 28pt を超える。ペットの下で押しやすく） |
| アイコン | 単色の線アイコン（1.6px 線）、色は `--label` |
| 押したとき | 面を `--surface-press` に。拡大・波紋は使わない |
| 動き | 出るときは 150ms で不透明度 0→1、`--ease`。`prefers-reduced-motion` では動かさない |
| 色付け | 色はチャットの送信ボタンだけ（`--tint`）。他は単色 |

## 部品

- **バー**: ペットの下中央に浮くカプセル。左＝チャット（ペン）、右＝音声（波形）、間に縦の区切り線
- **チャット入力**: 幅 360px のカプセル。左に「＋」、中に入力欄（プレースホルダ「新しいチャットを開始」）、右に丸い送信ボタン（`--tint`）
- **右クリックメニュー**: 角丸 12px の島。行は 32px、左にアイコン、右にラベル。項目は「設定」「バーを隠す」「位置をリセット」「終了」。区切り線は「終了」の前だけ
- **設定**: 普通の窓（タイトルバーあり）。Settings 風のグループ表示。行は 44px

## 禁止

- 紫→青のグラデーション、ネオンの縁、意味のないガラス
- 1px の灰色枠で囲んだカード
- アイコンに色を付けること（送信ボタン以外）
- 純白の文字を暗い面に置くこと（`#f5f5f7` を使う）
