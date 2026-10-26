# Claude Code の対話（TUI）を ConPTY で自動操作して、画面の文字を記録する。
# 承認の E2E（e2e-hooks.sh）は `claude -p` で済んだが、質問（AskUserQuestion）は `-p` ではモデルが呼ばない
# （2026-10-27 司令塔の実測）ので、対話モードを人の代わりに動かす。
#
# 使い方:
#   python claude-tui.py --cwd <プロジェクト> --out <記録ファイル> --expect <正規表現> [--timeout 秒]
#                        [--env NAME=VALUE ...] -- <claude の引数...>
#   例: python claude-tui.py --cwd tmp/askq-free --out out.txt --expect "ANSWER=" -- --model sonnet "Use AskUserQuestion ..."
# 依存: pywinpty（`py -3 -m venv tmp/pty-venv; tmp/pty-venv/Scripts/pip install pywinpty`。tmp/ は gitignore 済み）
# 振る舞い:
#   - フォルダの信頼の確認（「Yes, I trust this folder」）が出たら ↓ と Enter で通す
#   - `--expect` の正規表現が画面（ANSI を除いた文字）に現れたら 0 で終わる。時間切れは 2
#   - 終わる前に /exit を送り、残れば殺す。記録ファイルには ANSI を除いた全文を書く
#   - 子の環境から CLAUDECODE・CLAUDE_CODE_ENTRYPOINT を外す（この道具を Claude Code の中から起こしても、
#     子は独立した対話の claude になる＝hook の origin は `cli`）
import argparse
import os
import re
import sys
import threading
import time

ANSI = re.compile(r"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b[()][A-Za-z0-9]|\x1b[=>]|\r")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--cwd", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--expect", required=True)
    ap.add_argument("--timeout", type=float, default=240)
    ap.add_argument("--env", action="append", default=[])
    ap.add_argument("rest", nargs=argparse.REMAINDER)
    a = ap.parse_args()
    args = a.rest[1:] if a.rest[:1] == ["--"] else a.rest

    from winpty import PtyProcess

    env = dict(os.environ)
    # 親の Claude Code の印（CLAUDECODE・CLAUDE_CODE_ENTRYPOINT・CLAUDE_CODE_CHILD_SESSION など）を外す。
    # 残すと子が「入れ子」になり、起動経路が親のものを引き継ぎ、会話の保存も止まる（2026-10-27 実測）
    for k in list(env):
        if k.startswith("CLAUDE"):
            env.pop(k)
    for kv in a.env:
        k, _, v = kv.partition("=")
        env[k] = v
    npm = os.path.join(os.environ.get("APPDATA", ""), "npm", "claude.cmd")
    cmd = ["cmd.exe", "/d", "/c", npm, *args]
    proc = PtyProcess.spawn(cmd, cwd=os.path.abspath(a.cwd), env=env, dimensions=(40, 140))

    chunks: list[str] = []
    lock = threading.Lock()

    def reader() -> None:
        while True:
            try:
                data = proc.read(4096)
            except (EOFError, OSError):
                return
            if not data:
                return
            with lock:
                chunks.append(data)

    threading.Thread(target=reader, daemon=True).start()

    def screen() -> str:
        with lock:
            raw = "".join(chunks)
        # ConPTY は語の間の空白を「カーソルを列 n へ」（ESC[nG）や「右へ n」（ESC[nC）で描く（2026-10-27 実測）。
        # 読める記録と語の照合のため、空白 1 つに戻してから ANSI を除く（列の正確な再現はしない）
        raw = re.sub(r"\x1b\[\d*[GC]", " ", raw)
        return ANSI.sub("", raw)

    expect = re.compile(a.expect)
    deadline = time.monotonic() + a.timeout
    trusted = False
    result = 2
    while time.monotonic() < deadline:
        time.sleep(0.5)
        text = screen()
        if not trusted and "Yes, I trust this folder" in text:
            proc.write("\x1b[B")
            time.sleep(0.3)
            proc.write("\r")
            trusted = True
            continue
        if expect.search(text):
            result = 0
            break
        if not proc.isalive():
            result = 3
            break
    # 答えの続き（1〜2 行）が描かれるのを少し待ってから終える
    time.sleep(2)
    try:
        if proc.isalive():
            # 質問の選択の UI が出たままなら Esc で取り消してから終える（Enter を送ると先頭の選択肢を選んでしまう。2026-10-27 実測）
            proc.write("\x1b")
            time.sleep(0.5)
            proc.write("/exit\r")
            for _ in range(20):
                if not proc.isalive():
                    break
                time.sleep(0.25)
    except OSError:
        pass
    try:
        if proc.isalive():
            proc.terminate(force=True)
    except OSError:
        pass
    with open(a.out, "w", encoding="utf-8") as f:
        f.write(screen())
    with lock, open(a.out + ".raw", "w", encoding="utf-8") as f:
        f.write("".join(chunks))
    print(f"result={result} chars={len(screen())} out={a.out}")
    return result


if __name__ == "__main__":
    sys.exit(main())
