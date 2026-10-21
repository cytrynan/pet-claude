"""e2e-hooks.sh 用: 作業用の settings.json に、試験のコマンドだけを確認対象にする ask ルールを足す。

K のグローバル設定で Bash が許可されていると、-p でも承認の要求（PermissionRequest）が起きない。
ask は allow より強いので、この 1 コマンドだけは必ず承認の流れを通る。
"""

import json
import sys

path = sys.argv[1]
with open(path, encoding="utf-8") as f:
    settings = json.load(f)
settings["permissions"] = {"ask": ["Bash(echo pet-claude-e2e:*)"]}
with open(path, "w", encoding="utf-8") as f:
    json.dump(settings, f, indent=2, ensure_ascii=False)
