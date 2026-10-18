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


def build_prompt(args, rng, diff_body):
    t = TEMPLATE.read_text(encoding="utf-8")
    focus = "\n".join(f"- {x}" for x in args.focus) if args.focus else "（特になし。差分全体を見る）"
    gates = (read_files(args.gates, "品質ゲート出力") if args.gates
             else "（添付なし。司令塔がまだ実行していないか、対象外）")
    # 差分の中に {{...}} が含まれても壊れないよう、差分は最後に1回だけ置換する
    for k, v in (("{{RANGE}}", rng), ("{{FOCUS}}", focus),
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
    ap.add_argument("--model", help="Codex のモデルを明示する場合")
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
    out.mkdir(parents=True, exist_ok=True)
    result_path, log_path = out / "result.md", out / "codex-log.txt"
    # 同じ --out を使い回したとき、前回の回答を今回の回答と取り違えない（途中で終了しても残さない）
    for stale in (result_path, log_path, out / "summary.json", out / "status-before.txt", out / "status-after.txt"):
        stale.unlink(missing_ok=True)

    rng, diff_body, files, truncated = collect_diff(repo, args)
    prompt_path = out / "prompt.md"
    prompt_path.write_text(build_prompt(args, rng, diff_body), encoding="utf-8")
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

    full =[*cmd, "exec", "-s", "read-only", "--ephemeral", "-C", str(repo), "-o", str(result_path)]
    if args.model:
        full += ["-m", args.model]
    full.append("-")
    print(f"[codex-verify] Codex 実行中（制限 {args.timeout} 秒）…", flush=True)
    t0 = datetime.datetime.now()
    timed_out = False
    try:
        with open(log_path, "wb") as log, open(prompt_path, "rb") as stdin:
            # POSIX では新しいセッションにして、時間切れのときプロセスグループごと止められるようにする
            proc = subprocess.Popen(full, stdin=stdin, stdout=log, stderr=subprocess.STDOUT,
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
    log_text = log_path.read_text(encoding="utf-8", errors="replace")
    summary = {"repo": str(repo), "range": rng, "files": files, "diff_truncated": truncated, "exit_code": rc, "seconds": secs,
               "timed_out": timed_out, "tree_unchanged": before == after,
               "codex_could_run_commands": not any(m in log_text for m in CMD_UNAVAILABLE_MARKERS),
               "prompt": str(prompt_path), "result": str(result_path), "log": str(log_path)}

    code = 0
    if before != after:
        (out / "status-before.txt").write_text(before_status, encoding="utf-8")
        (out / "status-after.txt").write_text(after_status, encoding="utf-8")
        print("[codex-verify] !!! Codex 実行の前後で作業ツリーが変わった。結果を採らず、"
              "git status / git diff で何が変わったか確認して人に報告すること。", file=sys.stderr)
        code = 3
    text = result_path.read_text(encoding="utf-8", errors="replace") if result_path.exists() else ""
    if code == 0 and (timed_out or rc != 0 or not text.strip()):
        print(f"[codex-verify] 結果が得られなかった（timeout={timed_out}, rc={rc}）。ログ: {log_path}",
              file=sys.stderr)
        code = 4
    summary.update(parse_result(text))
    summary["status"] = {0: "ok", 3: "tree_modified", 4: "no_result"}[code]
    (out / "summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8")

    print(f"[codex-verify] 所要 {secs} 秒 / 作業ツリー無変化: {summary['tree_unchanged']} / "
          f"Codex のコマンド実行: {'可' if summary['codex_could_run_commands'] else '不可（読むレビューのみ）'}")
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
