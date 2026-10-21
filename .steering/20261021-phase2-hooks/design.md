---
type: Steering
description: Phase 2（Claude Code の hooks 連携と、吹き出しからの承認）の設計。中心ロジックの要求は requirements.md、ここは exe・サーバー・吹き出し・登録・検証の設計
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-21T11:15:00+09:00 }
---

# Phase 2 の設計

K 指示: 「Phase 2（Claude Code の hooks 連携と、吹き出しからの承認の許可・拒否）に進んで」。

## 部品

| 部品 | 場所 | 役割 |
|---|---|---|
| 中心ロジック | `pet-core::hook`・`pet-core::ipc` | 入力の解析・要約・出力の組み立て・HTTP の最小の読み書き（requirements.md。テスト先行） |
| hook の実行ファイル | `crates/pet-hook`（新規・bin） | 標準入力を読み、アプリへ送る。承認だけは答えを待って標準出力に返す。**どんな失敗でも終了コード 0** |
| 受け口 | `pet-app/src/ipc_server.rs` | `127.0.0.1:0` で待ち受ける別スレッド。発見ファイルを書く。届いた出来事はメインスレッドへ回す |
| 吹き出し | `pet-app/src/ui.rs`＋`ui/bubble.html` | ペットの上に出す。承認のカード（許可・拒否・ターミナルで）と、短い固定文言 |
| 登録 | `pet-app.exe --install-hooks [--settings <path>]`・`--uninstall-hooks` | settings.json に印付きで足す・外す。足す前にバックアップ |

## 流れ

```
Claude Code ─ hook ─▶ pet-hook.exe ── POST /event ─────────────▶ pet-app（受け口のスレッド）── defer ─▶ Behavior.on_agent
                          │                                                                      └─▶ 吹き出し（固定文言）
                          └─ 承認: POST /permission → {"id"} ─▶ 承認の表に Pending ── defer ─▶ 吹き出しにカード
                             GET /permission/<id> を 300ms ごと ◀─ allow / deny / ask（ボタン）
                             → 標準出力に permission_output(...)、終了コード 0
```

- 発見ファイル: `%LOCALAPPDATA%\pet-claude\runtime\ipc.json`（`Discovery`）。起動時に一時ファイル＋rename で書き、終了時に消す。token は 32 バイトの乱数（`BCryptGenRandom`）の 16 進
- 試験用に `PET_CLAUDE_DATA_DIR` で `%LOCALAPPDATA%\pet-claude` の場所を差し替えられる（アプリと hook の両方）
- hook の時間: 接続 300ms・応答 500ms。アプリが居なければ即座に終わる。承認は最大 `PET_PERMISSION_WAIT_SECS`（既定 55 秒）待って、答えが無ければ `{}`（いつもの流れ）
- 承認の表は id → 状態。答えた後に 1 回読まれたら消す。120 秒で期限切れ
- 受け口は `is_authorized` を通らない要求に 401、`Origin` 付きは 403（`is_authorized` が false を返すので、どちらかは問わない）

## 吹き出し

- 承認のカード: 見出し「許可しますか？」、ツール名、要約（`summary`・HTML はエスケープ）、ボタン「許可」（色付き）「拒否」「ターミナルで」。**フォーカスを取らない**。外をクリックしても閉じない（答えるまで残る）。複数あれば古い順に 1 枚ずつ
- 固定文言: 完了「できたよ」、入力待ち（承認以外）「入力を待ってるよ」、失敗「失敗しちゃった」。3 秒で消える。バー・メニューが出ている間は出さない
- 承認中のペットのポーズは `NeedsInput`（手を振る）

## hooks の登録内容

| hook | async | timeout | 理由 |
|---|---|---|---|
| `UserPromptSubmit`・`PreToolUse`・`Notification`・`Stop`・`StopFailure` | true | 5 | 結果を返さない。Claude Code を待たせない |
| `PermissionRequest` | false | 60 | 答えを返すため待つ |

コマンドは exec 形式（`"command": "<pet-hook.exe の絶対パス>", "args": ["--pet-claude-managed"]`）。`--pet-claude-managed` を含む既存の項目を先に消してから足す（何度実行しても 1 つ）。`~/.claude/settings.json` への実際の登録は K の了承を得てから。

## 検証（K の入力を使わない）

1. 中心ロジック: テスト先行（テスト作成係 Opus・実装係 Sonnet。Fable は利用上限）＋ミューテーション
2. hook の故障注入（`crates/pet-hook/tests/`、ビルドした exe を起動する）: アプリが居ない／token が違う／受け口が応答しない／入力が壊れている → いずれも 200ms 以内（応答しない場合は 500ms＋α）に終了コード 0、承認なら `{}`
3. 端から端まで: 作業用の空フォルダの `.claude/settings.json` だけに hooks を登録し、そこで `claude -p --model haiku` に「`echo pet-claude-e2e` を実行して」と頼む。承認の画面を出せないので `PermissionRequest` が動く（一次資料）。アプリは試験用の `PET_AUTO_DECIDE=allow|deny`（デバッグビルドだけ）で、ボタンと同じ関数を 1.5 秒後に呼ぶ。allow なら出力に `pet-claude-e2e`、deny なら実行されないこと、ask（何もしない）なら拒否扱い
4. 吹き出しの見た目は撮影だけ（クリックしない）。ボタンを実際に押す確認は、K が PC を使っていない時に 1 回
