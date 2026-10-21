---
type: Architecture
description: pet-claude の技術仕様。Rust 1 プロセス。ペット本体は Win32 の layered window、吹き出し・設定などの UI は Tauri 2（WebView2）。入力（hooks）・行動・描画・声を分離する
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-21T08:05:00+09:00 }
---

# pet-claude 技術仕様書

## 1. 技術選定（2026-10-21 改訂: 未導入の言語も候補に入れて選び直した）

| 候補 | 判定 | 理由 |
|---|---|---|
| **Rust：ペット本体＝Win32 layered window、UI＝Tauri 2** | **採用** | `UpdateLayeredWindow` に画素ごとのアルファを渡すと、アルファ 0 の画素へのクリックを OS が背後へ通す。追加の判定も監視も要らない。`WS_EX_NOACTIVATE` でフォーカスを確実に奪わない。常駐は専有 20〜24MB（ワーキングセット 43〜60MB）・CPU 0.6%（Phase 1 実測、デバッグビルド、UI を出していない間。専有＝子孫プロセス込みの `PrivateMemorySize64`）。吹き出し・バー・チャット・設定は Tauri の WebView で作る。WebView2 は 1 枚でもあると子プロセスが常駐する（UI 表示中は専有 115〜147MB・ワーキングセット 330〜400MB）ので、UI の窓は使うときだけ作り、隠して 60 秒で破棄する（§3-b）。hook も Rust の小さな exe にでき、起動が速い |
| Python 3.12 + PySide6 | 不採用（当初案） | 窓は作れるが、凝った UI（Apple 風の吹き出し・ガラス）が Qt では重い。hook を Python で起動すると遅い |
| C# / WPF | 次点 | layered window と DPI は良い。UI は XAML で作れる。ただし Acrylic を透過窓に重ねにくく、Web 系 UI のほうが作りやすい |
| Electron（OpenPets 方式） | 不採用 | Windows ではクリックの forward が止まり、750ms ごとの監視で再武装が要る。常駐が重い |

声の ML（ローカル STT・ウェイクワード）は Phase 3〜4 で決める。第 1 候補は Rust 内で動かす方式（whisper-rs、`ort` で openWakeWord の ONNX を動かす）。精度や手間で劣るなら、Python のサイドカー（faster-whisper・openWakeWord）を別プロセスで動かす。どちらでも、アプリとは HTTP で疎結合にする。

## 2. 全体構成

```
Claude Code ──hook(stdin JSON)──▶ pet-hook.exe（Phase 2）──HTTP(loopback+token)──▶ ┐
                                    ▲ PermissionRequest だけ同期で待つ                │
                                    └──────────── decision ◀────────────────────── │
                                                                                     ▼
              pet-app.exe（1 プロセス・トレイ常駐・単一インスタンス・メインスレッド 1 本）
              ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────────┐  ┌────────┐
              │ inputs       │─▶│ behavior     │─▶│ pet_window（Win32 layered）  │  │ voice  │
              │ hooks / 手動 │  │ 状態機械     │  │ ui（Tauri: 吹き出し・設定）  │  │ TTS/STT│
              └──────────────┘  └──────────────┘  └──────────────────────────────┘  └────────┘
```

| crate | 役割 | OS 依存 |
|---|---|---|
| `crates/pet-core` | 置き場所（複数画面・クランプ・保存）、スプライト規約、仮画像の生成。Phase 1 で behavior（状態機械）、Phase 2 で hook の解析とサニタイズを足す | なし（`cargo test` で全部検証） |
| `crates/pet-app` | Win32 のペット窓、ディスプレイ列挙、トレイ（Tauri）、状態の保存、ログ | Windows |
| `crates/pet-hook`（Phase 2） | Claude Code から呼ばれる hook。常に exit 0 | Windows |

## 3. ペットの窓（`crates/pet-app/src/pet_window.rs`）

| 要件 | 実装 | Phase 0 の結果 |
|---|---|---|
| 透過・枠なし | `WS_POPUP` + `WS_EX_LAYERED`。`UpdateLayeredWindow(ULW_ALPHA)` に BGRA 事前乗算済みの DIB を渡す | 合格 |
| 最前面・タスクバー/Alt+Tab 非表示 | `WS_EX_TOPMOST` + `WS_EX_TOOLWINDOW` | 合格 |
| フォーカスを奪わない | `WS_EX_NOACTIVATE`、`WM_MOUSEACTIVATE` に `MA_NOACTIVATE` を返す、`SW_SHOWNOACTIVATE` で表示 | 合格 |
| 不透明部分だけクリック | アルファ 0 の画素は OS が背後へ通す。縁のかすかな影を拾わないよう、アルファ 24 未満は 0 に落とす | 合格 |
| ドラッグ | `WM_LBUTTONDOWN` で `SetCapture`。4px 以上動いたらドラッグ扱いにして「つままれ」ポーズ。座標は `GetMessagePos`（メッセージ発生時点。`GetCursorPos` だと速いドラッグで掴んだ位置がずれた） | 合格 |
| 画面外に出ない | ドラッグ中はカーソルの居る画面の作業領域に、離したら最も重なる画面の作業領域にクランプ | 合格 |
| 倍率の違う画面 | Per-Monitor DPI v2。窓の大きさ＝192×208 × `scale` × DPI/96。画面をまたいだら足元（下端中央）を基準に描き直す | 合格 |
| 位置の保存 | 画面ごとに足元を保存（鍵は物理 bounds `"x,y,WxH"`、最大 8 件）。`home` は最後に置いた画面で、その画面が外れている間も消さない | 合格（単体テスト＋実機で外して付け直し） |
| 画面構成の変化 | `WM_DISPLAYCHANGE`・`WM_DPICHANGED`・`WM_SETTINGCHANGE(SPI_SETWORKAREA)` を 200ms まとめて置き直す。ドラッグ中は保留 | 合格（`DisplaySwitch` でノートだけ→拡張に戻して実測） |
| 全画面アプリ | 1 秒ごとに `HWND_TOPMOST` を取り直す。ただし全画面の窓が前面の間は譲る | 合格 |
| 二重起動 | 名前付きミューテックス `Local\pet-claude-single-instance` | 合格 |

窓はメインスレッド（Tauri のイベントループと同じスレッド）で作るので、Tauri のメッセージループがそのまま配送する。状態は `thread_local` に置く。ウィンドウプロシージャは Win32 API の中から再入される（`ReleaseCapture` → `WM_CAPTURECHANGED` など）ため、借用中に来たメッセージは既定処理に回す。

## 3-b. ペットの周りの UI（Phase 1・`crates/pet-app/src/ui.rs`・`ui/*.html`）

見た目の正本は `DESIGN.md`。窓はすべて Tauri の WebView。**使うときに作り、全部が隠れて 60 秒たったら破棄する**（4 枚を常に持つと WebView2 の子プロセス 9 個で専有 335MB だった。2026-10-21 実測）。出すときは位置→大きさの順に設定する（画面をまたぐと DPI の調整が先に入るため）。

- WebView2 の起動引数: `--renderer-process-limit=1 --disable-gpu`（描画プロセスを 1 つにまとめ、GPU プロセスを止める。4 枚を常に持つ構成で専有 335→160MB。見た目は撮影で同じ）
- 窓を作る・出す処理はすべて `ui::defer` でイベントループのキューに積む。WebView2 の窓の生成は完了を入れ子のメッセージループで待つので、ペットのウィンドウプロシージャや WebView2 のコールバックの中で作ると、借用中の状態や再入とぶつかる（設定の窓が真っ白になった 1 回は、コマンドの中で作った版だった）
- 表示中かどうかは自分で記録する（毎フレームの問い合わせを減らすため。`is_visible()` が直後に古い値を返すかは確かめていない）
- 最後の窓が閉じても終了しない（`RunEvent::ExitRequested` の終了コード無しを `prevent_exit`）。メニュー・トレイの「終了」は `app.exit(0)` で通る

| 窓 | 出し方 | フォーカス | 閉じ方 |
|---|---|---|---|
| バー（`bar`） | ペットをクリック。ペットの下・中央 | 取らない（`focusable(false)`） | もう一度ペットをクリック／外をクリック／メニュー「バーを隠す」 |
| チャット入力（`chat`） | バーの「チャット」。バーと同じ場所 | 取る（文字を打つため。利用者が選んだとき） | フォーカスが外れる／Esc／外をクリック |
| 右クリックメニュー（`menu`） | ペットを右クリック。カーソルの位置 | 取らない | 項目を押す／外をクリック |
| 設定（`settings`） | メニュー「設定」かトレイ「設定」 | 取る（普通の窓） | 窓を閉じる（破棄） |

- フォーカスを取らない窓は Esc もフォーカスの喪失も来ないので、**開いている間だけ低レベルのマウスフック（`WH_MOUSE_LL`）を入れ**、島とペットの外でボタンが押されたら閉じる。閉じる処理はフックの中でせず、イベントループに回す
- 窓は島より四方 12px 大きく取り、その中に影を描く（透過窓の外に影は出せない）
- ペットは毎 tick `ui::popups_visible()` を読み、出ている間は `Behavior::set_held(true)`（散歩も睡眠もしない）
- UI からの操作は Tauri のコマンド（`ui_action`・`chat_submit`・`settings_get`・`settings_set`）。コマンドはメインスレッドで動く
- **後回しは `ui::defer`（別スレッドから `run_on_main_thread`）**。`run_on_main_thread` はメインスレッドから呼ぶと**その場で同期実行される**（tauri-runtime-wry 2.12.2 の `send_user_message`）ので、メインスレッドから直接呼んでも後回しにならない。窓の生成・外側クリックの処理・借用中のペットへの操作はすべて `ui::defer` を通す（2026-10-21 検証係の指摘で判明）

## 4. 行動（Phase 1・`pet-core::behavior`）

### 4.1 入力の型

```rust
pub enum AgentEvent {
    Prompt { session: String },
    ToolStart { session: String },
    NeedsInput { session: String },
    Done { session: String },
    Error { session: String },
}
```

### 4.2 ポーズと優先順位

正本は `.steering/20261021-phase1-behavior/requirements.md`（境界値・乱数・無操作の時計の定義まで決めてある）。ここは要約。

```
Grabbed（ドラッグ中）
 > Reaction（クリック・完了: Jumping／失敗: Failed を 1 回だけ）
 > Agent: needs_input(Waiting) > working(Running)
 > Autonomous: Sleep > Walk(RunLeft/RunRight) > Idle
```

- 散歩は設定「放置すると散歩する」（既定オフ・K 指示 2026-10-21）がオンのときだけ。オフのときは `walk_after_ms` を `u64::MAX` にして散歩に入らない（睡眠はそのまま）。切り替えたら Behavior を作り直す
- 自律行動は `Idle` が基本。無操作 90 秒〜で散歩（作業領域の端で折り返す）、5 分〜で睡眠。散歩は「散歩（3〜8 秒）と休み（10〜30 秒）」を交互に繰り返す。Agent 状態が来たら自律行動は中断
- Agent 状態は最後のイベントから 120 秒で `Idle` に戻す（hook が落ちても固まらない）
- 複数セッションはセッションごとに状態を持ち、`needs_input > working` の順で代表を選ぶ。失敗は状態でなく 1 回きりの反応

### 4.3 スプライトの行（`pet-core::sprite`）

| 行 | ポーズ | 代替（行が無いとき） |
|---|---|---|
| 0 Idle / 1 RunRight / 2 RunLeft / 3 Waving / 4 Jumping / 5 Failed / 6 Waiting / 7 Running / 8 Review | Codex 互換 | — |
| 9 Sleep | 独自 | Idle を半分の速さで |
| 10 Grabbed | 独自 | Waiting |

`--pet <dir>` で `pet.json`＋スプライトシート（PNG/WebP）を読む。指定が無ければ内蔵の仮画像（`assets/pets/placeholder/`、`cargo run -p pet-core --example gen_assets` で再生成）。

## 5. Claude Code 連携（Phase 2）

### 5.1 登録する hook

| hook | 送る `AgentEvent` | 同期 |
|---|---|---|
| `UserPromptSubmit` | Prompt | 非同期（`async: true`, timeout 3） |
| `PreToolUse`（`*`） | ToolStart（ツール名のみ） | 非同期 |
| `PermissionRequest`（`*`） | Permission（ツール名＋要約） | **同期**（timeout 60）。返事が来るまで待つ |
| `Notification` | NeedsInput（承認以外の入力待ち） | 非同期 |
| `Stop` | Done | 非同期 |
| `StopFailure` | Error | 非同期 |

`~/.claude/settings.json` へは `--pet-claude-managed` マーカー付きで追記し、追記前にバックアップを作る。`CLAUDE.md` には書かない。

### 5.2 承認の往復

1. hook が `POST /permission` でアプリに登録し、`GET /permission/<id>` を 500ms ごとに最大 60 秒ポーリング
2. アプリは吹き出しに「許可」「拒否」を出し、押されたら返事を保存
3. hook は返事を `hookSpecificOutput`（`hookEventName: "PermissionRequest"`、`decision.behavior: "allow"|"deny"`）で stdout に返す
4. 60 秒以内に返事が無ければ `{}` を返して端末の通常プロンプトに戻す。アプリが居なければ即 `{}`

`decision` の正確なスキーマは実装時に公式ドキュメントで再確認する。

### 5.3 IPC

- アプリ起動時に `127.0.0.1` の空きポートで HTTP を待ち受け、`%LOCALAPPDATA%\pet-claude\runtime\ipc.json` に `{endpoint, token, pid}` を書く
- hook は token を `Authorization` に付ける。接続 300ms・応答 500ms で諦めて exit 0
- 間引きは `%LOCALAPPDATA%\pet-claude\runtime\throttle.json`（同じ種類は 10 秒）

## 6. 声（Phase 3〜4）

| 部品 | 実装 | 無いとき |
|---|---|---|
| TTS | AivisSpeech `POST /audio_query` → `POST /synthesis`（`127.0.0.1:10101`、天深シノ・ノーマル。ai-news の `dev\ai-news-routine\lib\radio.mjs` と同じエンジンと話者） | 起動していなければ黙って無効 |
| STT | whisper.cpp（whisper-rs）か faster-whisper（サイドカー）。日本語・ローカル | 設定画面で案内 |
| ウェイクワード | openWakeWord（`ort` かサイドカー）。K の録音で独自モデル | 既定 OFF |
| チャット | Claude Agent SDK（PRD U1 の決定待ち） | — |

話す文言は固定文言＋検証済みの短文のみ（1〜140 字・単一行・コード/URL/パス/秘密らしい文字列は拒否）。

## 7. 設定・ログ・保存先

- `%LOCALAPPDATA%\pet-claude\state.json`: `scale`（0.25〜1.5、既定 0.55）、`positions`。書き込みは一時ファイル＋rename
- `%LOCALAPPDATA%\pet-claude\logs\pet.log`: 起動・配置・クリック・ドラッグ・DPI 変化。`PET_TRACE=1` でドラッグ中の座標も書く
- 自動起動は Windows タスク（`run-hidden.vbs` 経由）。Phase 4

## 8. 検証

- `bash scripts/gate.sh`: `cargo fmt --check` → `cargo clippy -D warnings` → `cargo test`。CI も同じ内容を `windows-latest` で回す
- 実機の窓まわりは `crates/pet-app/examples/winctl.rs` で再実行できる（窓の位置・クリック・ドラッグ・撮影・画面変化の通知）。手順と結果は `.steering/20261021-phase0-window/`
- ⚠️ ゲートは clippy とテストだけで、実行ファイルを作り直さない。実機で確かめる前に `cargo build -p pet-app --all-targets` を走らせる（Phase 0 で、古い exe のまま測りかけた）
