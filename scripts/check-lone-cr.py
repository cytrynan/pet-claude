"""テキストに「LF の付かない単独の CR」が無いことを確かめる（scripts/gate.sh から呼ぶ）。

CRLF は core.autocrlf=true の作業ツリーで普通に現れ、commit 時に git が LF へ直すので害は無い。
単独の CR は直されずにそのまま入る（2026-10-21、Python 文字列の \\r が CR に化けて docs に無言で
混入した実例。評価係の指摘）。追跡・未追跡（.gitignore 対象を除く）のテキストファイルを全部見る。
"""

import re
import subprocess
import sys

files = subprocess.run(
    ["git", "ls-files", "-co", "--exclude-standard", "-z"],
    capture_output=True, check=True,
).stdout.decode("utf-8").split("\0")
lone_cr = re.compile(rb"\r(?!\n)")
bad = []
for path in filter(None, files):
    try:
        with open(path, "rb") as f:
            data = f.read()
    except OSError:
        continue  # 削除済みなど
    if b"\0" in data[:8192]:
        continue  # バイナリ
    if lone_cr.search(data):
        bad.append(path)
if bad:
    print("単独の CR を含むファイル:")
    print("\n".join(bad))
    sys.exit(1)
