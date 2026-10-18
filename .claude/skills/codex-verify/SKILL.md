---
name: codex-verify
description: 標準フローの検証工程として、実装後の差分を OpenAI Codex CLI に読み取り専用で検証させ、結果を回収する。同梱スクリプトが差分の収集、依頼文のひな形への差し込み、Codex の起動（リポジトリは書き込み不可のまま、テスト・lint・型検査は Codex 自身が実行できる隔離形）、Codex 実行の前後で作業ツリーが変わっていないかの機械チェック、指摘件数の集計までを1コマンドで行う。次のような依頼で使う:「Codex で検証して」「検証工程を回して」「この差分を Codex にレビューさせて」「実装が終わったので検証に出して」、標準フロー（計画→実装→検証）で実装係の作業が終わったとき。逆に、計画段階の設計書レビュー（Fable のレビュー係のまま）、コードを書き換えさせる作業、git 管理外のファイルの検証には使わない。
---

# codex-verify — 差分を Codex に検証させる

正本: `orchestration` スキルの「検証係を Codex CLI に任せる」節。
検証係を別ベンダーにすることで、「司令塔と検証係を同一モデルにしない」原則が自動で満たされる。

## 手順

1. **品質ゲートの分担を決める**。Codex はテスト・lint・型検査を自分で再実行できる（2026-10-18 実測。下の「仕組みと注意」）。
   - Node のリポジトリ（vitest・tsc・eslint）は **Codex に任せてよい**。指定しなければ Codex が package.json から選ぶ。決めたいときは `--run` で渡す。
   - Codex で動かないもの（ユーザーの AppData に入った Python・pwsh、tsx、ネットワークが要るテスト）は、**司令塔が実行して生出力を `--gates` で渡す**。
     ```bash
     python -m pytest > "$TMP/gates-test.txt" 2>&1
     ```
2. **スクリプトで Codex に検証させる**（スキルをユーザー単位に置いた場合はパスを `~/.claude/skills/...` に読み替える）。見てほしい点は `--focus` で具体的に書く。
   計画から変えた箇所、並行しうる経路、データを消しうる処理などを書く。
   ```bash
   python .claude/skills/codex-verify/scripts/codex_verify.py --repo <repo> --base origin/main \
     --focus "計画から変えた箇所: ○○" --run "npx.cmd vitest run --configLoader runner" --run "npm.cmd run lint" \
     --spec <repo>/.steering/<作業単位>/requirements.md
   ```
   - `--run` は Codex に実行させるコマンド、`--gates` は司令塔が実行した生出力のファイル（どちらも複数可）。両方あれば Codex が食い違いも確かめる。
   - 範囲の指定は3通り。省略すると未コミットの変更と未追跡ファイルを対象にする。`--base <ref>` はブランチ全体、`--commit <sha>` は1コミット分を対象にする。
   - 結果は一時フォルダの `codex-verify/<repo名>-<日時>/` に保存する。`--out` で場所を指定できるが、リポジトリの中も、リポジトリを含むフォルダも不可（結果フォルダの下の作業場は毎回空にするため）。
   - 保存されるファイルは7つ。`prompt.md` は実際の依頼文、`result.md` は Codex の回答（「実行したコマンド」節に終了コードつき）、`commands.md` は Codex が実際に実行したコマンドと終了コード・出力の末尾、`codex-events.jsonl` は `--json` のイベント列、`codex-log.txt` は実行ログ（標準エラー）、`codex-cmd.txt` は Codex の起動コマンド、`summary.json` は件数などの要約。
   - 依頼文だけ確かめたいときは `--dry-run` を付ける。小さな差分でテストを回して約1分。長いときは Bash のタイムアウトを延ばすか、バックグラウンドで実行する。
   - `summary.json` の `codex_could_run_commands` が false なら、コマンドが起動できていない（読むレビューだけになった）。品質ゲートは司令塔が回して `--gates` で渡し直す。
3. **終了コードで分岐する**。

   | コード | 意味 | 次の行動 |
   |---|---|---|
   | 0 | 検証完了 | 手順4へ |
   | 2 | Codex が使えない（未導入・未ログイン・起動失敗・利用枠切れ。理由は画面に出る。利用枠切れは `summary.json` の `codex_errors` にも残る。一時的な rate limit は 4 側） | `readonly-reviewer` を**司令塔と別モデル**で立て、代行したことを記録に残す |
   | 3 | Codex 実行の前後で作業ツリーが変わった | 結果を採らずに止める。`status-before.txt` と `status-after.txt`、`git diff` で変化を確かめて人に報告する |
   | 4 | 結果が得られなかった（タイムアウト・空の回答・異常終了） | `codex-log.txt` を読み、1回だけ再実行する。だめなら終了コード2と同じ代替に回す。「codex を止められない」と出たら、プロセスを止めてから `git status` を確かめる |
   | 5 | 引数・リポジトリのエラー（差分が空、`--out` がリポジトリ内など） | 引数を直す |

4. **結果を受理する**。`result.md` を読み、指摘ごとに司令塔が該当コードを開いて正しいか再確認してから採る。
   - 「指摘なし」も鵜呑みにしない。壊しにいくケースを最低1本、司令塔が自作して流す。
   - 採った指摘は実装係に差し戻す。直したら、同じコマンドで再検証する。
   - Codex が報告したテスト結果も鵜呑みにしない。`commands.md` で本当に実行したか・終了コードを確かめ、commit の前に司令塔が品質ゲートをもう一度実行する。
5. **記録する**。実案件で使ったら、指摘件数・正誤・所要時間を実測表に1行追記する。Claude の検証係と精度を比べるため。

## 仕組みと注意

- **起動形（隔離形・既定）**: `codex exec --ignore-user-config --skip-git-repo-check -s workspace-write --ephemeral -C <結果フォルダ>/codex-work -c windows.sandbox="elevated" -c model=… -c model_reasoning_effort=… --json -o result.md -`。依頼文は標準入力から渡す。
  - 作業場（cwd）は結果フォルダ側の空フォルダ。**リポジトリは作業場の外なので、シェルでも apply_patch でも書き込みは拒否される**（EPERM・`patch rejected: writing outside of the project`）。読むことと、コマンドの実行はできる。
  - 環境変数: `TEMP`/`TMP` を `<結果フォルダ>/codex-tmp` へ（vitest などの一時ファイル用）、`LOCALAPPDATA` を空フォルダへ（下記）、`GIT_CONFIG_*` で対象リポジトリを `safe.directory` に足す（コマンドは別ユーザーで動き、git が所有者違いで止まるため）。
  - `--ignore-user-config` で外れる設定のうち、`model` と `model_reasoning_effort` だけは `~/.codex/config.toml` から読んで `-c` で渡し直す（外したままだと推論強度が none になる）。`--model` を付ければそちらが優先。
  - `--dangerously-bypass-approvals-and-sandbox` は使わない。
- **なぜこの形か（Windows・Codex デスクトップアプリを入れた PC で 2026-10-18 実測）**: 既定の起動（ユーザー設定のまま `-s read-only`）では、サンドボックス準備が Codex 同梱の `node_repl.exe` に ACL を付けようとして os error 32（使用中）で失敗し、コマンドが全滅する。
  ユーザー設定の `mcp_servers.node_repl` で Codex 自身が node_repl を起動するのと、**Codex デスクトップアプリが起動中だとアプリの node_repl** が掴んでいるのが原因。
  前者は `--ignore-user-config`、後者は `LOCALAPPDATA` を空フォルダへ向けて runtime の探索先から外すことで避けられる。詳細は `codex-interop` スキルの注意節。
- **Codex のコマンド実行環境の制約**（Windows で実測。依頼文にも書く。Windows 以外では OS 固有の指示は出さず、未実測）: コマンドはサンドボックス用の別ユーザー（`CodexSandboxOffline`）で動き、ネットワークは無い。
  ユーザーの AppData に入れた Python・pwsh・rg は起動できない。Program Files の node・git は動く。PowerShell は 5.1 の ConstrainedLanguage で（`commands.md` のコマンド欄は `pwsh.exe -Command …` と出るが、サンドボックスのログでは `powershell.exe` で起動されており `$PSVersionTable` も 5.1 だった）、`.ps1`（`npm.ps1` を含む）は実行ポリシーで止まるので `npm.cmd`・`npx.cmd` で呼ぶ。
  vitest は `--configLoader runner` を付ける（付けないと設定ファイルの一時変換を `node_modules/.vite-temp` に書こうとして EPERM）。tsx は `os.userInfo()` が失敗して動かない。
- `--readonly-sandbox` を付けると旧来の形（ユーザー設定のまま `-s read-only -C <repo>`）で起動する。隔離形が将来の Codex 更新で動かなくなったときの退避用で、Windows ではコマンドが動かないことがある。
- **差分は依頼文に埋め込む**。Codex が git を実行できなくても検証が成立するようにするため。
  150,000 文字を超える差分は先頭だけを埋め込み、警告を出して `summary.json` の `diff_truncated` を true にする。
  省略部分は検証されていないので、`--commit` などで範囲を分けて流し直す。
- `--commit` は第一親との差分を渡す。マージコミットでも取り込んだ変更が漏れない。
- **前後比較のガード**: サンドボックスがリポジトリへの書き込みを拒否することは実測済みだが、それに頼り切らず、Codex の実行前後で作業ツリーの指紋を取って比べる。
  指紋の材料は、git status、追跡ファイルの差分、未追跡ファイルの中身、`.gitignore` 対象ファイルのパスとサイズと更新時刻。
- **コマンドの実行状況は `--json` のイベント列（`codex-events.jsonl`）から読む**。コマンド1本が1件の `command_execution`（`exit_code`・`status`・出力つき）で来る。
  テキストのログ（`codex-log.txt`）を文字列で探すのはやめた。ログの先頭には依頼文がそのまま出て、読んだソースや過去のログもコマンド出力として出るので、誤検知と見逃しが避けられなかった。
  - `codex_could_run_commands` は、`exit_code=-1` の出力にサンドボックスのエラーがあるか、試みたコマンドが1本も終了コードを返さなければ false。
    false なら、少なくとも一部のコマンドが起動に失敗している。true でも、コマンドが成功した証明にはならない。
  - `commands_attempted`・`commands_finished` は、試みた本数と、終了コードを返した本数（成否は問わない）。
  - `commands.md` は、実行したコマンドごとの終了コードと出力の末尾。Codex の「テストを回した」という報告をここで確かめる。
- 前後比較は HEAD のコミットとステージの中身も見る。Codex が `reset` や `checkout` をしても気づける。
- 既知の制限: サブモジュールの中は指紋に入れていない。すでに変更のあるサブモジュール内のファイルを、さらに書き換えられると気づけない。
  サブモジュールを使うリポジトリでは、実行後にサブモジュール側でも `git status` を確かめる。
- 既知の制限: `.gitignore` 対象のファイルは中身を読まず、サイズと更新時刻で比べる。両方を保ったまま中身だけ書き換えられると気づけない。
- 既知の制限: `--base` で、分岐点にあったファイルを削除コミットした後に未追跡で戻すと、削除と新規追加の両方が差分に出る。
  情報は欠けないので、そのまま読ませてよい。
- 指摘件数は回答の `### N. <ファイル>:<行> — 重大度: 高|中|低` という見出しを数えている。形式が崩れていると0件と出るので、そのときは `result.md` を直接読む。
- 依頼文のひな形は `templates/prompt.md`。差し込み口は `{{RANGE}}`・`{{FOCUS}}`・`{{SPECS}}`・`{{ENV}}`・`{{RUN}}`・`{{GATES}}`・`{{DIFF}}` の7つ。
- 動作確認用に、環境変数 `CODEX_BIN` で codex の代わりの実行ファイルを指定できる。故障注入のテストは `python scripts/test_codex_verify.py`
  （偽 codex で終了コード 0/2/3/4/5・起動引数・環境変数・集計を確かめる。本物の Codex は起動しない）。スクリプトを直したら必ず流す。
