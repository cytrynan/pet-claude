---
name: tasklist-to-issues
description: |
  .steering/<YYYYMMDD>-<タスク名>/tasklist.md から GitHub issue（親1件＋子N件）を一括起票し、
  tasklist.md に issue 番号を書き戻す。さらに issue 側の変更（追加・クローズ）と tasklist の
  食い違いを検出する。作業計画（steering モード1）の直後・実装着手前に必ず使う。
  次のような時に使う: tasklist.md を作った直後 / 「issueに上げて」「issue起票して」/
  セッション開始時に進行中の作業単位があり issue 側の変更を取り込みたい時 /
  「#12 やって」と番号で指示され対応する作業単位を特定したい時。
  逆に、tasklist.md が無い軽微な作業（1〜2ファイルの修正・質問への回答）、GitHub リモートが無い
  リポジトリ、issue 以外の一般的な GitHub 操作（PR作成・レビュー）には使わない。
allowed-tools: Read, Write, Edit, Bash, Glob, Grep
---

# tasklist.md → GitHub issue 一括起票

狙い: ユーザーが「Claude Code にタスクを聞いて指示する」「GitHub の issue を見て指示する」の
**どちらからでも同じ作業に入れる**状態を保つこと。運用ルールは `CLAUDE.md` と `steering` スキルに準拠する。

## 使うタイミング

| 場面 | やること |
|---|---|
| 計画直後・実装着手前 | `new-issues-from-tasklist.ps1` で起票（**必須**） |
| セッション開始時／指示受領時 | `check-tasklist-issues.ps1` で食い違いを確認し、issue 側の変更を取り込む |
| タスク着手時・完了時 | tasklist.md を Edit した**直後に**対応する issue も更新（下記コマンド） |

## 手順1: 起票（実装着手前）

```powershell
# まず必ず DryRun で対象を確認する
.\.claude\skills\tasklist-to-issues\scripts\new-issues-from-tasklist.ps1 `
    -TasklistPath ".steering\20260728-機能名\tasklist.md" -DryRun

# 内容に問題なければ本実行
.\.claude\skills\tasklist-to-issues\scripts\new-issues-from-tasklist.ps1 `
    -TasklistPath ".steering\20260728-機能名\tasklist.md"
```

> PowerShell が無い環境では、下の「スクリプトがやること」を `gh` コマンドで手順どおり実行する。

スクリプトがやること:

1. `gh repo view` でリモートを確認（**通らなければ何もせず終了** → 起票をスキップした旨を1行報告して tasklist.md だけで進める）
2. ラベル `task/<unit>` / `tracking` / `in-progress` を無ければ作成（**ラベルが無いと `--label` は失敗する**）
   - **GitHub のラベル名は 50 文字が上限**。`task/<YYYYMMDD>-<長いタスク名>` は簡単に超え、
     超えると `gh label create` が `HTTP 422: Validation Failed` で落ちる（本文に文字数の話は出ない）。
     50 文字に詰めて警告を出す。`-Label` で明示指定も可。突合側も同じ詰め方をする
3. 親issue `[tracking] <unit>` を作成（`requirements.md` の先頭行を目的として引用）
4. トップレベルのタスク行ごとに子issueを作成（サブタスクを**受入条件**として本文に展開、`Part of #<親>`）
5. `tasklist.md` に `tracking: #<親>` と各行末の ` (#<子>)` を書き戻す
6. 親issue本文のチェックリストを更新

安全側の作り:

- 既に `(#NN)` が付いた行・`~~` でスキップ済みの行・コードフェンス内・`{プレースホルダ}` は対象外 → **再実行しても二重起票しない**
- 計画エージェントが置く **`(#TBD)`（行末）と `tracking: #TBD`** はそのまま渡してよい。タイトルから外して起票し、書き戻しで実番号に置き換える（2 本目の `tracking:` 行は増えない。2026-10-02）
- 起票対象が **10件を超えると停止**する（`-MaxTasks` / `-Force` で変更可）。細切れの大量起票は issue 一覧を読めなくし、「issue を見て指示する」目的自体を壊すため。停止したらタスクを階層化して親タスク単位にまとめる
- issue 本文は UTF-8(BOM無し) の一時ファイル経由（`--body-file`）。日本語・改行を `--body` にインラインで渡さない

## 手順2: 実装中の同期（その場で・まとめない）

| tasklist.md | issue 側のコマンド |
|---|---|
| `- [ ]` → `- [~]` 着手 | `gh issue edit <N> --add-label in-progress --add-assignee @me` |
| `- [~]` → `- [x]` 完了 | `gh issue close <N> --comment "<何を変えたか1〜3行>"` |
| `- [x] ~~名~~（理由）` スキップ | `gh issue close <N> --reason "not planned" --comment "<技術的理由>"` |

## 手順3: issue → tasklist の取り込み

```powershell
.\.claude\skills\tasklist-to-issues\scripts\check-tasklist-issues.ps1 `
    -TasklistPath ".steering\20260728-機能名\tasklist.md"
```

| Kind | 意味 | 対処 |
|---|---|---|
| `ISSUE_ONLY` | issue にあって tasklist に無い | ユーザーが追加したタスク。`- [ ] タイトル (#NN)` を追記して着手対象に含める |
| `CLOSED_OPEN` | issue closed / tasklist 未完了 | tasklist を `[x]` にする（**勝手に再実施しない**） |
| `OPEN_DONE` | issue open / tasklist `[x]` | 差し戻しの可能性。**どちらも上書きせずユーザーに確認する** |
| `NO_ISSUE` | tasklist にあって issue 番号が無い | 手順1で起票する |
| `LOCAL_ONLY` | tasklist の番号に対応する issue が無い | 番号の誤り・ラベル違いを確認する |

状態（open/closed・タスクの増減）は **issue を上流**とみなす。ユーザーが手で触るのは issue 側だから。
ただし**設計と受入条件の正本は `.steering/`**（`design.md` / `tasklist.md`）。

## 「#12 やって」と番号で指示された場合

```powershell
gh issue view 12 --json title,body,labels
```

ラベル `task/<YYYYMMDD>-<タスク名>` から作業単位ディレクトリを特定し、
**`tasklist.md` と `design.md` を読んでから**着手する。issue 本文だけを頼りに実装しない（issue は要約）。

## 実測メモ（2026-07-28 実運用リポジトリで確認）

DryRun → 本実行 → 食い違い検出5種 → 後片付けまで通した。親1件＋子3件の起票、`tracking:` 行の挿入、
行末 `(#NN)` の付与、親本文のチェックリスト反映、除外判定、再実行時の二重起票なしを確認。

ハマった点（Windows / PowerShell 5.1）:

1. **`.ps1` は UTF-8 BOM付きで保存する。** BOM無しだと 5.1 が ANSI 解釈して日本語が化け、引用符が壊れて構文エラーの山になる。
   エディタやツールで書き換えたら BOM を付け直す。
2. **`ConvertFrom-Json` はパイプラインで配列を列挙しない。** `@($json | ConvertFrom-Json) | Where-Object {...}` は
   件数が常に1になる。いったん変数に受けてから `@($var)` で配列化する。
3. **`gh repo view` はリポジトリを位置引数で取る**（`--repo` は無い）。他の `gh issue *` は `--repo` が使える。
4. `gh issue create` の戻りは issue URL のみ（`--json` 不可）。番号は URL 末尾から取る。
5. **`gh issue delete` は repo admin でも org ポリシーで拒否されることがある**
   （`Viewer not authorized to delete`）。テスト起票の後片付けは「タイトルに `[TEST]` を付けて close」までと想定する。
