"""生成した PDF を機械で確かめる（PyMuPDF）。目視の前に回す。

- ページ数と、各ページの先頭の文字（章の見出しが 8 つそろっているか）
- フォントが全部埋め込まれているか（埋め込みが無いと別の PC で日本語が化ける）
- 画像の枚数
- 目次のリンク（内部リンク）が実在するページを指しているか。外部 URL は無いこと

使い方: python docs/manual/check-pdf.py [pdf のパス]。問題があれば終了コード 1。
"""
import sys

sys.stdout.reconfigure(encoding="utf-8")
from pathlib import Path

import fitz  # PyMuPDF

CHAPTERS = ["1. はじめに", "2. 基本の操作", "3. Claude Code との連携", "4. チャット", "5. 音声入力と読み上げ",
            "6. 設定画面", "7. ペットの切り替えと追加", "8. 困ったとき"]


def main() -> int:
    pdf = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).with_name("pet-claude-manual.pdf")
    doc = fitz.open(pdf)
    bad = []
    text_all = ""
    fonts = {}
    images = 0
    links_internal = 0
    for i, page in enumerate(doc):
        text = page.get_text()
        text_all += text
        for f in page.get_fonts(full=True):
            # (xref, ext, type, basefont, name, encoding, referencer)。Type3 は Edge が字形をそのまま書いた
            # フォント（basefont が空）で、外のフォントに頼らないので埋め込み扱い
            key = f[3] or f"Type3:{f[4]}"
            fonts[key] = fonts.get(key, False) or f[1] != "n/a" or f[2] == "Type3"
        images += len(page.get_images(full=True))
        for link in page.get_links():
            if link.get("kind") in (fitz.LINK_GOTO, fitz.LINK_NAMED):
                links_internal += 1
                if not (0 <= link.get("page", -1) < doc.page_count):
                    bad.append(f"page {i + 1}: 内部リンクの先が無い {link}")
            elif link.get("kind") == fitz.LINK_URI:
                bad.append(f"page {i + 1}: 外部 URL のリンク {link.get('uri')}")
    for ch in CHAPTERS:
        if ch not in text_all:
            bad.append(f"章の見出しが無い: {ch}")
    not_embedded = [name for name, emb in fonts.items() if not emb]
    if not_embedded:
        bad.append(f"埋め込まれていないフォント: {not_embedded}")
    print(f"pages={doc.page_count} images={images} internal_links={links_internal} fonts={sorted(fonts)}")
    for i, page in enumerate(doc):
        head = page.get_text().strip().splitlines()[:1]
        print(f"  p{i + 1}: {head[0][:40] if head else '(文字なし)'}")
    for b in bad:
        print("NG", b)
    print("OK" if not bad else "FAIL")
    return 0 if not bad else 1


if __name__ == "__main__":
    sys.exit(main())
