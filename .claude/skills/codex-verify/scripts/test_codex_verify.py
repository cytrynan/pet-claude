#!/usr/bin/env python3
"""codex_verify.py の故障注入テスト。偽の codex（CODEX_BIN）で終了コードと起動引数を確かめる。

実行: python ~/.claude/skills/codex-verify/scripts/test_codex_verify.py
本物の Codex は起動しない。一時フォルダに git リポジトリと偽 codex を作り、finally で消す。
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

for _s in (sys.stdout, sys.stderr):
    try:
        _s.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass

SCRIPT = Path(__file__).resolve().parent / "codex_verify.py"

# 偽 codex。FAKE_MODE で振る舞いを変え、受け取った引数と TEMP を FAKE_RECORD に書く
FAKE = r'''
import json, os, sys
from pathlib import Path
a = sys.argv[1:]
mode = os.environ.get("FAKE_MODE", "ok")
if a[:2] == ["login", "status"]:
    print("Not logged in" if mode == "nologin" else "Logged in using ChatGPT")
    sys.exit(1 if mode == "nologin" else 0)
prompt = sys.stdin.read()
sys.stderr.write("user\n" + prompt + "\n")  # 本物と同じく、テキストのログ（標準エラー）の先頭に依頼文をそのまま出す
Path(os.environ["FAKE_RECORD"]).write_text(json.dumps({"argv": a, "temp": os.environ.get("TEMP"), "lad": os.environ.get("LOCALAPPDATA"), "git": {k: v for k, v in os.environ.items() if k.startswith("GIT_CONFIG_")}}), encoding="utf-8")
out = Path(a[a.index("-o") + 1])
ERR = "Failed to create unified exec process: helper_unknown_error: setup refresh had errors"
OLD_LOG = "exec\ngit status in C:\\old\n exited -1 in 0ms:\n" + ERR   # 過去のテキストログ（出力に混ざる）


def ev(**item):   # --json の標準出力（1行1イベント）
    item.setdefault("type", "command_execution")
    print(json.dumps({"type": "item.completed", "item": item}))


print(json.dumps({"type": "thread.started", "thread_id": "x"}))
if mode == "modify":
    (Path(os.environ["FAKE_REPO"]) / "touched.txt").write_text("x", encoding="utf-8")
if mode == "marker":      # 本物の失敗の形
    ev(command="git status", aggregated_output=ERR, exit_code=-1, status="failed")
if mode == "listing":     # 成功したコマンドの出力に、エラー文字列や過去のログの見出しが出る（ソース・ログを読んだだけ）
    ev(command="Get-Content old.log", aggregated_output='MARKERS = ("setup refresh had errors",)\n' + OLD_LOG,
       exit_code=0, status="completed")
if mode == "multiline":   # 複数行コマンドの起動失敗（成功した1本の後）
    ev(command="git --version", aggregated_output="git version 2", exit_code=0, status="completed")
    ev(command='powershell -Command "Get-Date\nGet-Location"', aggregated_output=ERR, exit_code=-1, status="failed")
if mode == "otherfail":   # 形式の違う失敗で、1本も終了まで進まない
    ev(command="git status", aggregated_output="some new sandbox error", exit_code=-1, status="failed")
if mode == "ran":
    ev(command="npm.cmd test", aggregated_output="ok", exit_code=0, status="completed")
    ev(command="npx.cmd tsc", aggregated_output="err", exit_code=1, status="failed")
    ev(command="git status", aggregated_output="?", exit_code=-1, status="failed")   # 起動できなかった1本は数えない
    ev(type="agent_message", text="x")   # コマンド以外の item は数えない
if mode in ("credits", "apierror"):   # Codex 自体のエラー（回答なしで終了コード 1）
    msg = "Your workspace is out of credits." if mode == "credits" else "Rate limit reached. Please try again in 20s."
    print(json.dumps({"type": "error", "message": msg}))
    print(json.dumps({"type": "turn.failed", "error": {"message": msg}}))
    sys.exit(1)
if mode == "ok":          # テキストログにだけ失敗らしき文字列（イベントには無い）→ 誤検知しない
    sys.stderr.write(OLD_LOG + "\n")
if mode == "hang":       # 応答しない。PID を残して、時間切れで木ごと止められたかをテスト側で確かめる
    import time
    Path(os.environ["FAKE_PIDFILE"]).write_text(str(os.getpid()), encoding="utf-8")
    time.sleep(300)
if mode == "fail":
    sys.exit(1)
if mode != "empty":
    body = ("## 判定\n指摘あり 1 件\n\n## 指摘\n### 1. a.txt:1 — 重大度: 中\n- 内容: x\n" if mode == "findings"
            else "## 判定\n指摘なし\n\n## 実行したコマンド\n- なし\n\n## 確認できなかったこと\n- なし\n")
    out.write_text(body, encoding="utf-8")
'''


def base_env(**extra):
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_CONFIG_")}
    env.update(extra)
    return env


def alive(pidfile):
    """pidfile に書かれた PID のプロセスがまだ生きているか。"""
    try:
        pid = int(pidfile.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return False
    if os.name == "nt":
        r = subprocess.run(["tasklist", "/FI", f"PID eq {pid}", "/NH"], capture_output=True, text=True,
                           encoding="mbcs", errors="replace")
        return str(pid) in r.stdout
    try:
        os.kill(pid, 0)
        return True
    except OSError:
        return False


def run(cmd, cwd=None, env=None):
    return subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True, encoding="utf-8", errors="replace")


def main():
    root = Path(tempfile.mkdtemp(prefix="cv-test-"))
    fails = []
    try:
        repo = root / "repo"
        repo.mkdir()
        for c in (["init", "-q"], ["config", "user.email", "t@example.invalid"], ["config", "user.name", "t"]):
            run(["git", "-C", str(repo), *c])
        (repo / "a.txt").write_text("1\n", encoding="utf-8")
        run(["git", "-C", str(repo), "add", "."])
        run(["git", "-C", str(repo), "commit", "-qm", "init"])
        (repo / "a.txt").write_text("2\n", encoding="utf-8")   # 検証対象の未コミット変更

        fake_py = root / "fake_codex.py"
        fake_py.write_text(FAKE, encoding="utf-8")
        if os.name == "nt":
            fake = root / "codex.cmd"
            # cmd.exe はバッチを ANSI コードページで読むので、日本語パスを含む行は mbcs で書く
            fake.write_text(f'@"{sys.executable}" "{fake_py}" %*\r\n', encoding="mbcs")
        else:
            fake = root / "codex"
            fake.write_text(f'#!/bin/sh\nexec "{sys.executable}" "{fake_py}" "$@"\n', encoding="utf-8")
            fake.chmod(0o755)
        codex_home = root / "codex-home"
        codex_home.mkdir()
        (codex_home / "config.toml").write_text(
            'model = "fake-model"\nmodel_reasoning_effort = "high"\nnotify = ["x"]\n', encoding="utf-8")
        record = root / "record.json"

        def case(name, mode, want_code, extra=(), check=None):
            out = root / f"out-{name}"
            env = base_env(CODEX_BIN=str(fake), FAKE_MODE=mode, FAKE_RECORD=str(record),
                       FAKE_REPO=str(repo), CODEX_HOME=str(codex_home))
            record.unlink(missing_ok=True)
            r = run([sys.executable, str(SCRIPT), "--repo", str(repo), "--out", str(out), "--timeout", "60", *extra],
                    env=env)
            ok = r.returncode == want_code
            msg = ""
            if ok and check:
                msg = check(out) or ""
                ok = not msg
            print(f"{'PASS' if ok else 'FAIL'} {name}: 終了コード {r.returncode}（期待 {want_code}）{msg}")
            if not ok:
                fails.append(name)
                print(r.stdout[-1500:], r.stderr[-1500:], sep="\n")
            # modify ケースの後始末（次のケースの前後比較を汚さない）
            (repo / "touched.txt").unlink(missing_ok=True)

        def rec():
            return json.loads(record.read_text(encoding="utf-8"))

        def summary(out):
            return json.loads((out / "summary.json").read_text(encoding="utf-8"))

        def norm(argv):
            """-C と -o の値だけ実パスにそろえる（比較は引数列の完全一致で行い、余計な引数も拾う）。"""
            argv = list(argv)
            for flag in ("-C", "-o"):
                if flag in argv:
                    i = argv.index(flag) + 1
                    argv[i] = str(Path(argv[i]).resolve())
            return argv

        def check_isolated(out):
            a, s = rec(), summary(out)
            problems = []
            want = (["exec", "--ignore-user-config", "--skip-git-repo-check", "-s", "workspace-write", "--ephemeral",
                     "-C", str((out / "codex-work").resolve())]
                    + (["-c", 'windows.sandbox="elevated"'] if os.name == "nt" else [])
                    + ["-c", 'model="fake-model"', "-c", 'model_reasoning_effort="high"',
                       "--json", "-o", str((out / "result.md").resolve()), "-"])
            if norm(a["argv"]) != want:
                problems.append(f"起動引数が想定と違う（余計な引数・欠落を含む）: {a['argv']}")
            if Path(a["temp"]).resolve() != (out / "codex-tmp").resolve():
                problems.append(f"TEMP が作業用一時フォルダでない: {a['temp']}")
            if os.name == "nt" and Path(a["lad"]).resolve() != (out / "codex-tmp" / "localappdata").resolve():
                problems.append(f"LOCALAPPDATA を空フォルダへ向けていない: {a['lad']}")
            g = a["git"]
            if (g.get("GIT_CONFIG_COUNT") != "1" or g.get("GIT_CONFIG_KEY_0") != "safe.directory"
                    or g.get("GIT_CONFIG_VALUE_0") != repo.resolve().as_posix()):
                problems.append(f"safe.directory を渡していない: {g}")
            if s.get("sandbox") != "isolated-workspace" or not s["no_findings"] or not s["tree_unchanged"]:
                problems.append(f"summary が想定外: {s.get('sandbox')}, {s['no_findings']}, {s['tree_unchanged']}")
            prompt = (out / "prompt.md").read_text(encoding="utf-8")
            if "{{" in prompt.split("## 差分")[0] or str(repo.resolve()) not in prompt:
                problems.append("依頼文の差し込み漏れ、またはリポジトリの絶対パスが無い")
            return "; ".join(problems)

        def check_model_override(out):
            argv = rec()["argv"]
            if argv[argv.index("-m") + 1] != "cli-model" or 'model="fake-model"' in argv:
                return "--model が設定ファイルの model より優先されていない"

        def check_legacy(out):
            argv = rec()["argv"]
            problems = []
            if rec()["git"]:
                problems.append("旧来の形なのに GIT_CONFIG_* を渡した")
            want = ["exec", "-s", "read-only", "--ephemeral", "-C", str(repo.resolve()),
                    "--json", "-o", str((out / "result.md").resolve()), "-"]
            if norm(argv) != want:
                problems.append(f"旧来の read-only 形になっていない: {argv}")
            if summary(out).get("sandbox") != "readonly-legacy":
                problems.append("summary.sandbox が readonly-legacy でない")
            return "; ".join(problems)

        def check_run(out):
            prompt = (out / "prompt.md").read_text(encoding="utf-8")
            if "- `npx.cmd vitest run --configLoader runner`" not in prompt:
                return "--run のコマンドが依頼文に入っていない"

        def check_marker(out):
            if summary(out)["codex_could_run_commands"]:
                return "サンドボックス失敗の文字列があるのに codex_could_run_commands が true"

        def check_marker_in_prompt(out):
            if not summary(out)["codex_could_run_commands"]:
                return "依頼文（差分）に含まれる文字列を、ログのエコーからサンドボックス失敗と誤検知した"

        def check_ran(out):
            s = summary(out)
            cmds_md = (out / "commands.md").read_text(encoding="utf-8")
            if "npm.cmd test" not in cmds_md or "終了コード -1" not in cmds_md:
                return "commands.md にコマンドと終了コードが出ていない"
            if s["commands_attempted"] != 3 or s["commands_finished"] != 2 or not s["codex_could_run_commands"]:
                return f"attempted={s['commands_attempted']}（期待 3）, finished={s['commands_finished']}（期待 2）"

        def want_could_run(value, finished=None):
            def check(out):
                s = summary(out)
                if s["codex_could_run_commands"] != value or (finished is not None and s["commands_finished"] != finished):
                    return (f"codex_could_run_commands={s['codex_could_run_commands']}（期待 {value}）, "
                            f"commands_finished={s['commands_finished']}（期待 {finished}）")
            return check

        def check_findings(out):
            s = summary(out)
            if s["findings"] != 1 or s["severity"]["中"] != 1:
                return f"指摘件数が想定外: {s['findings']}"

        def check_stale(out):
            # 2回目の実行で、前回の作業場の残骸が消えている
            junk = out / "codex-work" / "junk.txt"
            junk.write_text("x", encoding="utf-8")
            env = base_env(CODEX_BIN=str(fake), FAKE_MODE="ok", FAKE_RECORD=str(record),
                       FAKE_REPO=str(repo), CODEX_HOME=str(codex_home))
            r = run([sys.executable, str(SCRIPT), "--repo", str(repo), "--out", str(out)], env=env)
            if r.returncode != 0 or junk.exists():
                return "作業場が毎回空にされていない"

        case("基本（隔離形の起動引数）", "ok", 0, check=check_isolated)
        case("--model が設定より優先", "ok", 0, ["--model", "cli-model"], check_model_override)
        case("--readonly-sandbox で旧来の形", "ok", 0, ["--readonly-sandbox"], check_legacy)
        case("--run が依頼文に入る", "ok", 0, ["--run", "npx.cmd vitest run --configLoader runner"], check_run)
        case("指摘の集計", "findings", 0, check=check_findings)
        case("サンドボックス失敗の検知", "marker", 0, check=check_marker)
        doc = repo / "doc.md"   # 失敗の文字列を含む文書を差分に入れる（未追跡なので依頼文に埋め込まれる）
        doc.write_text("- 全コマンドが `setup refresh had errors` で失敗する\n"
                       "Failed to create unified exec process: helper_unknown_error: setup refresh had errors\n",
                       encoding="utf-8")
        case("差分中の失敗文字列で誤検知しない", "ok", 0, check=check_marker_in_prompt)
        case("差分に同じ文字列があっても本物の失敗は拾う", "marker", 0, check=want_could_run(False, 0))
        doc.unlink()
        gates = root / "gates.txt"   # 過去のログを貼ったゲート出力（依頼文に入る）。今回の実行に数えない
        gates.write_text("exec\nnpm test in C:\\old\n succeeded in 1200ms:\n", encoding="utf-8")
        case("ゲート出力中の実行ログを数えない", "ok", 0, ["--gates", str(gates)], want_could_run(True, 0))
        case("成功したコマンドの出力中の文字列で誤検知しない", "listing", 0, check=want_could_run(True, 1))
        case("形式の違う失敗でも1本も進まなければ不可", "otherfail", 0, check=want_could_run(False, 0))
        case("複数行コマンドの起動失敗も拾う", "multiline", 0, check=want_could_run(False, 1))
        case("実行コマンド数の集計", "ran", 0, check=check_ran)
        case("作業場の持ち越し防止", "ok", 0, check=check_stale)
        case("作業ツリー改変→3", "modify", 3)
        case("回答が空→4", "empty", 4)
        case("codex 異常終了→4", "fail", 4)
        case("未ログイン→2", "nologin", 2)
        pidfile = root / "fake.pid"
        os.environ["FAKE_PIDFILE"] = str(pidfile)
        t0 = time.time()
        case("時間切れ→4（子プロセスの木ごと止める）", "hang", 4, ["--timeout", "5"],
             lambda out: (None if summary(out)["timed_out"] and time.time() - t0 < 120 and not alive(pidfile)
                          else f"timed_out={summary(out)['timed_out']}, 偽 codex が残っている={alive(pidfile)}"))

        def check_errors(want_status):
            def check(out):
                s = summary(out)
                if s["status"] != want_status or not s["codex_errors"]:
                    return f"status={s['status']}（期待 {want_status}）, codex_errors={s['codex_errors']}"
            return check

        case("利用枠切れ→2（再実行しない代替へ）", "credits", 2, check=check_errors("codex_unavailable"))
        case("一時的な rate limit→4（再実行側）", "apierror", 4, check=check_errors("no_result"))
        case("--out がリポジトリ内→5", "ok", 5, ["--out", str(repo / "out")])
        # 逆向き: リポジトリが結果フォルダの作業場の下にある → 作業場の初期化で消さずに 5 で止まる
        nest_out = root / "out-nest"
        nested = nest_out / "codex-work" / "proj"
        nested.mkdir(parents=True)
        run(["git", "-C", str(nested), "init", "-q"])
        (nested / "keep.txt").write_text("x", encoding="utf-8")
        r = run([sys.executable, str(SCRIPT), "--repo", str(nested), "--out", str(nest_out), "--dry-run"],
                env=base_env(CODEX_BIN=str(fake)))
        ok = r.returncode == 5 and (nested / "keep.txt").exists()
        print(f"{'PASS' if ok else 'FAIL'} --out がリポジトリを含む→5（リポジトリを消さない）: 終了コード {r.returncode}")
        if not ok:
            fails.append("out-contains-repo")
        r = run([sys.executable, str(SCRIPT), "--repo", str(repo), "--dry-run", "--out", str(root / "out-dry")],
                env=base_env(CODEX_BIN=str(root / "no-such-codex")))
        ok = r.returncode == 0 and (root / "out-dry" / "prompt.md").exists()
        print(f"{'PASS' if ok else 'FAIL'} --dry-run は codex を起動しない: 終了コード {r.returncode}")
        if not ok:
            fails.append("dry-run")
    finally:
        shutil.rmtree(root, ignore_errors=True)
    print(f"\n{'全件 PASS' if not fails else f'FAIL {len(fails)} 件: ' + ', '.join(fails)}")
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
