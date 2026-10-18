#!/usr/bin/env python3
"""codex-verify: 差分を OpenAI Codex CLI に読み取り専用で検証させ、結果を回収する。

終了コード:
  0 = 検証完了（指摘の有無は summary を見る）
  2 = Codex が使えない（未導入・未ログイン・起動失敗）→ readonly-reviewer を司令塔と別モデルで立てる
  3 = Codex 実行の前後で作業ツリーが変わった（読み取り専用が破られた疑い）→ 止めて人に報告
  4 = 結果が得られなかった（タイムアウト・空の回答）
  5 = 使い方・リポジトリのエラー
"""
import argparse
import datetime
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

for _s in (sys.stdout, sys.stderr):
    try:
        _s.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass

SKILL_DIR = Path(__file__).resolve().parent.parent
TEMPLATE = SKILL_DIR / "templates" / "prompt.md"
DIFF_EMBED_LIMIT = 150_000   # これを超える差分は埋め込まず、ファイル一覧だけ渡す
UNTRACKED_FILE_LIMIT = 30_000
CMD_UNAVAILABLE_MARKERS = ("setup refresh had errors", "blocked by policy",
                           "Failed to create unified exec process")
# Codex 自体のエラーのうち、再実行しても直らない利用枠切れ・上限（実測: "Your workspace is out of credits."）。
# 一時的な混雑（rate limit・429）は含めない（終了コード 4 で1回再実行する側に回す）
USAGE_LIMIT = re.compile(r"out of credits|usage limit|quota|billing", re.I)
# --ignore-user-config で外れるユーザー設定のうち、検証の質に効くものだけ引き継ぐ
CARRY_OVER_KEYS = ("model", "model_reasoning_effort")


def die(code, msg):
    print(f"[codex-verify] {msg}", file=sys.stderr)
    sys.exit(code)


def git(repo, *args, check=True):
    r = subprocess.run(["git", "-C", str(repo), *args], capture_output=True)
    if check and r.returncode != 0:
        die(5, f"git {' '.join(args)} が失敗: {r.stderr.decode('utf-8', 'replace').strip()}")
    return r.stdout.decode("utf-8", "replace")


def codex_cmd():
    env = os.environ.get("CODEX_BIN")
    if env:
        return [env]
    found = shutil.which("codex")
    if not found:
        die(2, "codex が見つからない（npm install -g @openai/codex）。"
               "readonly-reviewer を司令塔と別モデルで立てること。")
    return [found]


def check_login(cmd):
    try:
        r = subprocess.run([*cmd, "login", "status"], capture_output=True, timeout=60)
    except Exception as e:  # noqa: BLE001
        die(2, f"codex を起動できない: {e}")
    out = (r.stdout + r.stderr).decode("utf-8", "replace")
    if r.returncode != 0 or "Logged in" not in out:
        die(2, f"codex が未ログイン（対話端末で `codex login`）: {out.strip()[:200]}")


def user_config_overrides():
    """~/.codex/config.toml（CODEX_HOME）からモデル・推論強度を読み、-c で渡し直す。
    --ignore-user-config だけだと推論強度が none になり、検証の質が落ちる（2026-10-18 実測）。"""
    home = Path(os.environ.get("CODEX_HOME") or Path.home() / ".codex")
    try:
        import tomllib
    except ImportError:   # Python 3.10 以前
        print("[codex-verify] 警告: tomllib が無い（Python 3.11 未満）ため model・推論強度を引き継げない。"
              "Codex の既定（推論強度 none の場合あり）で動く。--model で明示するか Python を更新すること。", file=sys.stderr)
        return {}
    try:
        conf = tomllib.loads((home / "config.toml").read_text(encoding="utf-8"))
    except Exception:  # noqa: BLE001  設定が無い・読めないときは Codex の既定に任せる
        return {}
    return {k: conf[k] for k in CARRY_OVER_KEYS if isinstance(conf.get(k), str)}


def build_codex_args(cmd, args, repo, result_path, work_dir):
    """Codex の起動引数を作る。

    既定（隔離形）: ユーザー設定を読まず、作業場（cwd）を結果フォルダ側の空フォルダにして
    workspace-write で起動する。リポジトリは作業場の外なので書き込みは拒否され、読むことと
    コマンド実行はできる。一時ファイルは TEMP を作業場の隣へ向けて受ける（vitest などが書く）。
    Windows（Codex デスクトップアプリを入れた PC）では、ユーザー設定を読むとサンドボックス準備が node_repl.exe の os error 32 で失敗し、
    コマンドが全滅する。--ignore-user-config と windows.sandbox="elevated" で回避できる（2026-10-18 実測）。
    --readonly-sandbox: 旧来の形（ユーザー設定のまま -s read-only -C <repo>）。Windows ではコマンドが動かないことがある。
    """
    if args.readonly_sandbox:
        full = [*cmd, "exec", "-s", "read-only", "--ephemeral", "-C", str(repo)]
    else:
        full = [*cmd, "exec", "--ignore-user-config", "--skip-git-repo-check",
                "-s", "workspace-write", "--ephemeral", "-C", str(work_dir)]
        if os.name == "nt":
            full += ["-c", 'windows.sandbox="elevated"']
        for k, v in user_config_overrides().items():
            if k == "model" and args.model:
                continue
            full += ["-c", f"{k}={json.dumps(v)}"]
    full += ["--json", "-o", str(result_path)]   # --json: 実行したコマンドを構造化イベントで受け取る
    if args.model:
        full += ["-m", args.model]
    full.append("-")
    return full


def read_command_events(events_path):
    """`codex exec --json` のイベント列から、Codex が実行したコマンドを読む。

    テキストのログを文字列で探すと誤検知する（ログの先頭に依頼文がそのまま出る・読んだソースや過去のログが
    コマンド出力として出る）。イベント列ならコマンド1本が1件の item（exit_code・status つき）で来る。
    起動できなかったものは status=failed・exit_code=-1 で、出力にサンドボックスのエラーが入る（2026-10-18 実測）。
    戻り値は (コマンドの item の並び, Codex 自体のエラー文の並び)。利用枠切れなどは error / turn.failed で来る。"""
    cmds, errors = [], []
    try:
        lines = events_path.read_text(encoding="utf-8", errors="replace").splitlines()
    except OSError:
        return cmds, errors
    for line in lines:
        try:
            ev = json.loads(line)
        except ValueError:
            continue
        if not isinstance(ev, dict):
            continue
        item = ev.get("item")
        if ev.get("type") == "item.completed" and isinstance(item, dict) and item.get("type") == "command_execution":
            cmds.append(item)
        elif ev.get("type") == "error" and ev.get("message"):
            errors.append(str(ev["message"]))
        elif ev.get("type") == "turn.failed" and isinstance(ev.get("error"), dict):
            msg = str(ev["error"].get("message", ""))
            if msg and msg not in errors:
                errors.append(msg)
    return cmds, errors


def command_stats(cmds):
    """(試みた本数, 終了コードを返した本数, サンドボックス失敗の有無)。"""
    finished = sum(1 for c in cmds if isinstance(c.get("exit_code"), int) and c["exit_code"] >= 0)
    failed = any(c.get("exit_code") == -1 and any(m in str(c.get("aggregated_output", ""))
                                                   for m in CMD_UNAVAILABLE_MARKERS) for c in cmds)
    return len(cmds), finished, failed


def write_commands_md(cmds, path):
    """司令塔が「Codex が本当にテストを回したか」を確かめるための一覧（コマンド・終了コード・出力の末尾）。"""
    parts = ["# Codex が実行したコマンド（codex-events.jsonl から抽出）\n"]
    for i, c in enumerate(cmds, 1):
        tail = "\n".join(str(c.get("aggregated_output", "")).rstrip().splitlines()[-15:])
        parts.append(f"## {i}. 終了コード {c.get('exit_code')}（{c.get('status')}）\n\n"
                     f"```\n{c.get('command', '')}\n```\n\n出力の末尾:\n\n```\n{tail}\n```\n")
    if not cmds:
        parts.append("（なし）\n")
    path.write_text("\n".join(parts), encoding="utf-8")


def isolated_env(repo, tmp_dir):
    """隔離形で Codex に渡す環境変数。

    - TEMP/TMP/TMPDIR: 書ける一時フォルダ（vitest などが書く）。リポジトリの外に置く。
    - LOCALAPPDATA: 空のフォルダへ向ける。Codex デスクトップアプリが起動中だと、サンドボックス準備が
      %LOCALAPPDATA%\\OpenAI\\Codex\\runtimes の node_repl.exe（アプリが使用中）に ACL を付けようとして
      os error 32 で失敗し、コマンドが全滅する。探索先から外すと準備が通る（2026-10-18 実測）。
    - safe.directory: コマンドはサンドボックス用の別ユーザーで動くため、git が所有者違い
      （dubious ownership）で止まる。GIT_CONFIG_* で対象リポジトリだけを許可する。
    """
    env = dict(os.environ)
    env.update(TEMP=str(tmp_dir), TMP=str(tmp_dir), TMPDIR=str(tmp_dir))
    if os.name == "nt":
        lad = tmp_dir / "localappdata"
        lad.mkdir(exist_ok=True)
        env["LOCALAPPDATA"] = str(lad)
    try:
        n = int(env.get("GIT_CONFIG_COUNT") or 0)
    except ValueError:
        n = 0
    env.update({f"GIT_CONFIG_KEY_{n}": "safe.directory", f"GIT_CONFIG_VALUE_{n}": repo.as_posix(),
                "GIT_CONFIG_COUNT": str(n + 1)})
    return env


def snapshot(repo):
    """作業ツリーの指紋。git status・追跡ファイルの差分・未追跡ファイルの中身から作る。"""
    h = hashlib.sha256()
    status = git(repo, "status", "--porcelain=v1", "-uall")
    h.update(status.encode())
    # HEAD の付け替え（reset・checkout）とステージ内容の変化も拾う
    h.update(git(repo, "rev-parse", "--verify", "-q", "HEAD", check=False).encode())
    h.update(git(repo, "ls-files", "--stage", "-z").encode())
    h.update(subprocess.run(["git", "-C", str(repo), "diff", "HEAD", "--binary", "--no-ext-diff"],
                            capture_output=True).stdout)
    for f in git(repo, "ls-files", "--others", "--exclude-standard", "-z").split("\0"):
        if f:
            h.update(f.encode())
            try:
                h.update((Path(repo) / f).read_bytes())
            except OSError:
                h.update(b"<unreadable>")
    # .gitignore 対象（.env・ビルド成果物など）は中身を読まず、パス・サイズ・更新時刻で見る
    ignored = git(repo, "ls-files", "--others", "--ignored", "--exclude-standard", "-z").split("\0")
    for f in sorted(x for x in ignored if x):
        try:
            st = (Path(repo) / f).stat()
            h.update(f"{f}\0{st.st_size}\0{st.st_mtime_ns}\n".encode())
        except OSError:
            h.update(f"{f}\0<gone>\n".encode())
    return h.hexdigest(), status


def collect_diff(repo, args):
    opts = ["--no-color", "--no-ext-diff"]
    if args.commit:
        # 第一親との差分にする（マージコミットの combined diff だと変更が漏れる）。ルートコミットは --root
        sha = git(repo, "rev-parse", "--verify", f"{args.commit}^{{commit}}").strip()
        has_parent = subprocess.run(["git", "-C", str(repo), "rev-parse", "--verify", "-q", f"{sha}^1"],
                                    capture_output=True).returncode == 0
        rng = f"コミット {sha[:8]} が入れた変更（{'第一親との差分' if has_parent else 'ルートコミット'}）"
        if has_parent:
            diff = git(repo, "diff", *opts, f"{sha}^1", sha)
            files = git(repo, "diff", "--name-only", f"{sha}^1", sha)
        else:
            diff = git(repo, "show", *opts, "--root", "--format=", sha)
            files = git(repo, "show", "--root", "--name-only", "--format=", sha)
        if diff.strip():
            diff = "コミットメッセージ:\n" + git(repo, "log", "-1", "--format=%B", sha) + "\n" + diff
    elif args.base:
        # 分岐点から作業ツリーまでを1本の差分にする（コミット済みと未コミットを重ねて出さない）
        mb = git(repo, "merge-base", args.base, "HEAD").strip()
        rng = f"`{args.base}` からの変更（分岐点 {mb[:8]} から作業ツリーまで。未コミット分を含む）"
        diff = git(repo, "diff", *opts, mb)
        files = git(repo, "diff", "--name-only", mb)
    else:
        rng = "未コミットの変更（`git diff HEAD` ＋ 未追跡ファイル）"
        diff = git(repo, "diff", *opts, "HEAD")
        files = git(repo, "diff", "--name-only", "HEAD")
    untracked_cut = False
    if not args.commit:
        for f in git(repo, "ls-files", "--others", "--exclude-standard", "-z").split("\0"):
            if not f:
                continue
            try:
                data = (Path(repo) / f).read_bytes()
            except OSError:
                continue
            files += f + "\n"
            if b"\0" in data[:8000]:
                diff += f"\n--- 未追跡（バイナリ）: {f}\n"
            else:
                text = data.decode("utf-8", "replace")
                if len(text) > UNTRACKED_FILE_LIMIT:
                    text = (text[:UNTRACKED_FILE_LIMIT] + "\n…（以下省略。省略部分は検証済みと書かず、"
                            "必要ならファイルを直接読むか「確認できなかったこと」に挙げる）")
                    untracked_cut = True
                diff += f"\n--- /dev/null\n+++ 未追跡（新規ファイル）: {f}\n{text}\n"
    files = sorted({x for x in files.splitlines() if x.strip()})
    if not diff.strip():
        die(5, f"検証対象の差分が空（{rng}）。範囲指定を見直すこと。")
    truncated = len(diff) > DIFF_EMBED_LIMIT
    if truncated:
        body = ("```diff\n" + diff[:DIFF_EMBED_LIMIT] + "\n```\n\n"
                f"**差分が大きい（{len(diff):,} 文字）ため、先頭 {DIFF_EMBED_LIMIT:,} 文字だけを埋め込んだ。**"
                "以降の変更は見えていない。省略部分を検証済みと書かず、「確認できなかったこと」に挙げること。"
                "対象ファイルの一覧:\n" + "\n".join(f"- {x}" for x in files))
    else:
        body = "```diff\n" + diff + "\n```"
    return rng, body, files, truncated or untracked_cut


def read_files(paths, label):
    if not paths:
        return "（なし）"
    parts = []
    for p in paths:
        try:
            body = Path(p).read_text(encoding="utf-8", errors="replace")
        except OSError as e:
            die(5, f"{label}ファイルを読めない: {p}: {e}")
        parts.append(f"### {label}: {p}\n```\n{body}\n```")
    return "\n\n".join(parts)


def build_env_note(args, repo):
    if args.readonly_sandbox:
        return (f"- 検証対象のリポジトリ（作業フォルダ）は `{repo}`。読み取り専用サンドボックスのため、"
                "コマンドは実行できないことがある。実行できなかったら「確認できなかったこと」にそう書く。")
    common = [
        f"- 検証対象のリポジトリは `{repo}`。あなたの作業フォルダ（cwd）はその外にある空フォルダなので、"
        "ファイルは絶対パスで読み、コマンドは作業ディレクトリをリポジトリにして実行する。",
        "- リポジトリへの書き込みはサンドボックスが拒否する。書けるのは作業フォルダと一時フォルダ（TEMP/TMPDIR）だけ。",
    ]
    tail = ["- 書き込み失敗（EPERM 等）や上の制約で落ちたコマンドは、コードの不具合と区別して「確認できなかったこと」に書く。"]
    if os.name != "nt":
        # Windows 以外では実測していない。OS 固有の指示は出さない
        return "\n".join(common + tail)
    return "\n".join(common + [
        "- コマンドは別ユーザー（サンドボックス用）で動く。ユーザーの AppData に入れたツール"
        "（Python・pwsh など）は起動できないことがある。Program Files の node・git は動く。",
        "- git は動く（`.config/git/ignore` を読めない警告は無視してよい）。`rg` は無いので、ファイル探しは `git ls-files`・`Get-ChildItem` で行う。",
        "- PowerShell は ConstrainedLanguage で `.ps1` は実行ポリシーで止まる。npm/npx は `npm.cmd`・`npx.cmd` で呼ぶ。",
        "- vitest は `npx.cmd vitest run --configLoader runner`（設定ファイルの一時変換をリポジトリ内に書かせない）。"
        "tsx は `os.userInfo()` が失敗して動かない。",
    ] + tail)


def build_prompt(args, rng, diff_body, repo):
    t = TEMPLATE.read_text(encoding="utf-8")
    focus = "\n".join(f"- {x}" for x in args.focus) if args.focus else "（特になし。差分全体を見る）"
    gates = (read_files(args.gates, "品質ゲート出力") if args.gates
             else "（添付なし。司令塔がまだ実行していないか、対象外）")
    if args.run:
        run = "\n".join(f"- `{x}`" for x in args.run)
    elif args.readonly_sandbox:
        run = "（指定なし）"
    else:
        run = ("（指定なし。package.json などから、そのリポジトリのテスト・lint・型検査を選んで実行する。"
               "無ければ実行しなくてよい）")
    # 差分の中に {{...}} が含まれても壊れないよう、差分は最後に1回だけ置換する
    for k, v in (("{{RANGE}}", rng), ("{{FOCUS}}", focus), ("{{ENV}}", build_env_note(args, repo)),
                 ("{{RUN}}", run),
                 ("{{SPECS}}", read_files(args.spec, "仕様")), ("{{GATES}}", gates)):
        t = t.replace(k, v)
    return t.replace("{{DIFF}}", diff_body, 1)


def kill_tree(proc):
    """子プロセスごと止める。Windows は taskkill /T、それ以外はプロセスグループごと SIGKILL。"""
    if os.name == "nt":
        subprocess.run(["taskkill", "/T", "/F", "/PID", str(proc.pid)], capture_output=True)
    else:
        import signal
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass


def parse_result(text):
    findings = re.findall(r"^###\s*\d+\.\s*(.+?)\s*[—\-–]+\s*重大度[:：]\s*(高|中|低)", text, re.M)
    no_findings = bool(re.search(r"^##\s*判定\s*\n+\s*指摘なし\s*$", text, re.M))
    sev = {"高": 0, "中": 0, "低": 0}
    for _, s in findings:
        sev[s] += 1
    return {"findings": len(findings), "severity": sev, "no_findings": no_findings,
            "locations": [loc for loc, _ in findings]}


class Parser(argparse.ArgumentParser):
    """引数エラーを終了コード 5 にする（argparse 既定の 2 は「Codex が使えない」と衝突する）。"""

    def error(self, message):
        self.print_usage(sys.stderr)
        die(5, f"引数エラー: {message}")


def main():
    ap = Parser(description="Codex に差分を読み取り専用で検証させる")
    ap.add_argument("--repo", required=True, help="検証対象の git リポジトリ（ワークツリー）")
    g = ap.add_mutually_exclusive_group()
    g.add_argument("--base", help="比較元ブランチ／コミット（例: origin/main）")
    g.add_argument("--commit", help="このコミットが入れた変更だけを検証")
    ap.add_argument("--focus", action="append", default=[], help="司令塔が特に見てほしい点（複数可）")
    ap.add_argument("--gates", action="append", default=[], help="品質ゲート（テスト・lint）の生出力ファイル（複数可）")
    ap.add_argument("--spec", action="append", default=[], help="仕様・設計ファイル（複数可）")
    ap.add_argument("--out", help="結果の保存先フォルダ（既定: 一時フォルダ。リポジトリの中は不可）")
    ap.add_argument("--timeout", type=int, default=900, help="Codex の制限時間（秒・既定 900）")
    ap.add_argument("--model", help="Codex のモデルを明示する場合（既定は ~/.codex/config.toml の model）")
    ap.add_argument("--run", action="append", default=[],
                    help="Codex に実行させる品質ゲートのコマンド（複数可。例: \"npx.cmd vitest run --configLoader runner\"）")
    ap.add_argument("--readonly-sandbox", action="store_true",
                    help="旧来の起動形（ユーザー設定のまま -s read-only -C <repo>）。Windows ではコマンドが動かないことがある")
    ap.add_argument("--dry-run", action="store_true", help="依頼文だけ作って Codex は起動しない")
    args = ap.parse_args()

    repo = Path(args.repo).resolve()
    if not repo.is_dir():
        die(5, f"リポジトリが無い: {repo}")
    repo = Path(git(repo, "rev-parse", "--show-toplevel").strip()).resolve()

    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    out = (Path(args.out).resolve() if args.out
           else Path(tempfile.gettempdir()).resolve() / "codex-verify" / f"{repo.name}-{stamp}")
    if out == repo or repo in out.parents:
        die(5, f"--out はリポジトリの外にすること（前後比較が壊れる）: {out}")
    if out in repo.parents:
        # 逆向きの包含も拒否する。結果フォルダの下の作業場は毎回 rmtree するので、そこにリポジトリがあると消える
        die(5, f"--out にリポジトリを含むフォルダは指定できない（作業場の初期化でリポジトリを消しうる）: {out}")
    out.mkdir(parents=True, exist_ok=True)
    result_path, log_path = out / "result.md", out / "codex-log.txt"
    events_path, commands_path = out / "codex-events.jsonl", out / "commands.md"
    # 同じ --out を使い回したとき、前回の回答を今回の回答と取り違えない（途中で終了しても残さない）
    for stale in (result_path, log_path, events_path, commands_path, out / "summary.json",
                  out / "status-before.txt", out / "status-after.txt", out / "codex-cmd.txt"):
        stale.unlink(missing_ok=True)
    # Codex の作業場と一時フォルダ。前回の残りを持ち越さないよう毎回空にする
    work_dir, tmp_dir = out / "codex-work", out / "codex-tmp"
    for d in (work_dir, tmp_dir):
        shutil.rmtree(d, ignore_errors=True)
        d.mkdir()

    rng, diff_body, files, truncated = collect_diff(repo, args)
    prompt_path = out / "prompt.md"
    prompt_path.write_text(build_prompt(args, rng, diff_body, repo), encoding="utf-8")
    print(f"[codex-verify] 対象: {repo}\n[codex-verify] 範囲: {rng}（{len(files)} ファイル）\n"
          f"[codex-verify] 依頼文: {prompt_path}")
    if truncated:
        print("[codex-verify] 警告: 差分が大きく一部しか渡せない。--commit などで範囲を分け、複数回に分けること。",
              file=sys.stderr)
    if args.dry_run:
        print("[codex-verify] --dry-run のため Codex は起動しない")
        return 0

    cmd = codex_cmd()
    check_login(cmd)
    before, before_status = snapshot(repo)

    full = build_codex_args(cmd, args, repo, result_path, work_dir)
    env = isolated_env(repo, tmp_dir) if not args.readonly_sandbox else dict(os.environ)
    (out / "codex-cmd.txt").write_text(subprocess.list2cmdline(full) + "\n", encoding="utf-8")
    print(f"[codex-verify] Codex 実行中（制限 {args.timeout} 秒）…", flush=True)
    t0 = datetime.datetime.now()
    timed_out = False
    try:
        with open(log_path, "wb") as log, open(events_path, "wb") as events, open(prompt_path, "rb") as stdin:
            # 標準出力は --json のイベント列、標準エラーは実行ログ。
            # POSIX では新しいセッションにして、時間切れのときプロセスグループごと止められるようにする
            proc = subprocess.Popen(full, stdin=stdin, stdout=events, stderr=log, env=env,
                                    start_new_session=(os.name != "nt"))
            try:
                rc = proc.wait(timeout=args.timeout)
            except subprocess.TimeoutExpired:
                kill_tree(proc)
                rc, timed_out = -1, True
                try:
                    proc.wait(timeout=30)
                except subprocess.TimeoutExpired:
                    die(4, f"codex を止められない（PID {proc.pid}）。前後比較ができないので、"
                           "タスクマネージャーで止めてから git status を確認すること。")
    except OSError as e:
        die(2, f"codex を起動できない: {e}")
    secs = int((datetime.datetime.now() - t0).total_seconds())

    after, after_status = snapshot(repo)
    cmds, codex_errors = read_command_events(events_path)
    write_commands_md(cmds, commands_path)
    attempted, finished, cmd_failed = command_stats(cmds)
    summary = {"repo": str(repo), "range": rng, "files": files, "diff_truncated": truncated, "exit_code": rc, "seconds": secs,
               "timed_out": timed_out, "tree_unchanged": before == after,
               "sandbox": "readonly-legacy" if args.readonly_sandbox else "isolated-workspace",
               # 試みたのに1本も終了まで進まなければ、形式の違う失敗でも「実行できなかった」とみなす
               "codex_could_run_commands": not (cmd_failed or (attempted and not finished)),
               "commands_attempted": attempted,
               "commands_finished": finished,
               "codex_errors": codex_errors,
               "prompt": str(prompt_path), "result": str(result_path), "log": str(log_path),
               "events": str(events_path), "commands": str(commands_path)}

    code = 0
    if before != after:
        (out / "status-before.txt").write_text(before_status, encoding="utf-8")
        (out / "status-after.txt").write_text(after_status, encoding="utf-8")
        print("[codex-verify] !!! Codex 実行の前後で作業ツリーが変わった。結果を採らず、"
              "git status / git diff で何が変わったか確認して人に報告すること。", file=sys.stderr)
        code = 3
    text = result_path.read_text(encoding="utf-8", errors="replace") if result_path.exists() else ""
    if code == 0 and (timed_out or rc != 0 or not text.strip()):
        for msg in codex_errors:
            print(f"[codex-verify] Codex のエラー: {msg}", file=sys.stderr)
        if not timed_out and any(USAGE_LIMIT.search(m) for m in codex_errors):
            # 利用枠切れ・上限は再実行しても直らない → 「Codex が使えない」と同じ代替（readonly-reviewer）へ
            print("[codex-verify] Codex の利用枠・上限に達している。readonly-reviewer を司令塔と別モデルで立てること。",
                  file=sys.stderr)
            code = 2
        else:
            print(f"[codex-verify] 結果が得られなかった（timeout={timed_out}, rc={rc}）。"
                  f"ログ: {log_path}・{events_path}", file=sys.stderr)
            code = 4
    summary.update(parse_result(text))
    summary["status"] = {0: "ok", 2: "codex_unavailable", 3: "tree_modified", 4: "no_result"}[code]
    (out / "summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8")

    print(f"[codex-verify] 所要 {secs} 秒 / 作業ツリー無変化: {summary['tree_unchanged']} / "
          + ("Codex のコマンド実行: なし" if not attempted else
             f"Codex のコマンド実行: {'可' if summary['codex_could_run_commands'] else '不可（読むレビューのみ）'}"
             f"（試み {attempted} 本・終了まで進んだもの {finished} 本）"))
    if code == 0:
        if summary["no_findings"] and summary["findings"] == 0:
            print("[codex-verify] 判定: 指摘なし（鵜呑みにせず、壊しにいくケースを1本自作すること）")
        else:
            s = summary["severity"]
            print(f"[codex-verify] 判定: 指摘 {summary['findings']} 件（高 {s['高']} / 中 {s['中']} / 低 {s['低']}）"
                  + ("" if summary["findings"] else " ※形式どおりに数えられなかった。result.md を直接読むこと"))
    print(f"[codex-verify] 結果: {result_path}\n[codex-verify] 要約: {out / 'summary.json'}")
    return code


if __name__ == "__main__":
    sys.exit(main())
