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
| メニュー（`menu`） | ペットを右クリック、または**カーソルがペットの不透明な部分に乗って 200ms**（K 指示 2026-10-21。バー・チャット・メニューが出ている間は乗っても出さない）。ペットの横・カーソルの高さ | 取らない | 項目を押す／外をクリック／**乗って出したものは、ペットとメニューの外へ出て 400ms で閉じる**（低レベルのマウスフックで見る）。乗って出した後に右クリックすると、クリックで閉じるまで出たままになる |
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

### 4.4 ペットの素材と切り替え（2026-10-21 K 指示）

- ペット＝フォルダ（`pet.json`＋スプライトシート PNG/WebP）。`pet.json` は Codex 互換の `id`・`displayName`・`spritesheetPath` に、独自の `cellScale`（1 コマを 192×208 の何倍で描いたか。既定 1・上限 4）を足した
- 探す場所（先に見つかった id を使う）: `%LOCALAPPDATA%\pet-claude\pets\<id>\` → exe と同じフォルダの `pets\<id>\` → デバッグビルドだけリポジトリの `assets\pets\<id>\`。内蔵の仮キャラ（`placeholder`）は exe に埋め込む
- 起動時に試す順: `--pet <dir>`（指定したら、それが読めなければ起動しない）→ state.json の `pet` → フォルダにあるペット（見つかった順）→ 仮キャラ（クリックの抜けを試す絵なので最後の手段）。読めないペットは飛ばして次を試す（`pets::startup_order`）
- 設定画面の「ペット」で切り替える。足元の位置と大きさはそのまま。選択は state.json の `pet`
- **読み込みは 1 本だけ、別スレッドで**（`pet_window::kick_load`）: 切り替え・大きさの変更・画面の DPI の変化のどれも `kick_load` を呼ぶだけ。要るとき（選んだペットがまだ出ていない、または持っているシートより細かい解像度が要る）だけ読み込みを 1 本始め、終わったらメインスレッドで「まだ欲しいペットか」を見て差し替え、もう一度 `kick_load` で最新の選択と大きさに追いつく。続けて選び直しても最後に選んだものが勝ち、スライダーを動かしてもメインスレッドは止まらない（2026-10-21 検証係の指摘）。読めなかったペットは選択を表示中のペットに戻す（設定画面は選んでから 2.5 秒後に聞き直して選択欄を合わせる）。読み込みのスレッドが panic しても失敗として戻す。同じペットを細かく読み直すのに失敗したら、そのペットを選び直すまで細かくは読まない（何度も試さないため）
- **行の有無とコマ数はシートを測って決める**（`pet-core::sprite::SheetLayout`。embed-pet の解析で推された取り込み・2026-10-21）: 縮める前の元のシートで、各行を左から見て中身のあるコマ（アルファ 8 より濃い画素がある）を数え、最初の空のコマで止める。空の行は代わりの行で再生する（睡眠→待機を半分の速さ、つままれ→入力待ち、ほか→待機、待機も空なら中身のある最初の行）。空の行を再生すると窓が全部透明になり、右クリックも背後へ抜けてメニューから戻せないため。1 コマの長さは Codex の値のままで、数えたコマ数で 1 周する。中身のある行が 1 つも無いシートは読み込まない
- **メモリ**: 高精細のシートはそのまま持つと重い（まめドラゴンの `cellScale` 2 は展開して 56MB）。読み込んだらすぐ、いまの表示に要る解像度（大きさ × 最大の DPI / 96）までコマごとに縮め、**中身のあるコマだけを、不透明な範囲で切り抜いて持つ**（`pets::CellStore`。まめドラゴンは 88 コマ中 57 コマ）。もっと細かく要るときだけ元から読み直す。描いたコマの保存はいまのポーズの行だけ。実測（2026-10-21・UI を出していない間）: 既定の大きさ 55%・200% の画面で専有 24.8MB、K の大きさ 105%（1 コマ 404×437 が要り、元の 384×416 のまま持つ）で専有 31.6MB・持つコマ 27.7MB。切り抜く前は同じ 105% で 58.6MB と N1（50MB）を超えていた。大きさを下げても縮め直さない（読み込みの間は一時的に約 110MB）
- デバッグビルドでも依存クレートと pet-core は最適化する（`Cargo.toml` の `[profile.dev.package]`）。最適化なしでは、まめドラゴンの読み込みに 11.7 秒かかった（最適化して 1.3〜1.4 秒）

**状態ごとの静止画からシートを作る**: `cargo run --release -p pet-core --example build_pet -- <静止画のフォルダ> assets/pets/<id> <id> <表示名> [cellScale]`。ファイル名に状態の名前（idle・walk・working・attention・happy・confused・sleeping・dragged）を含む静止画を、`pet-core::compose` が同じ縮尺にそろえ、足元を下の中央に合わせ、コマごとに小さな動き（呼吸・上下・跳ねる・左右）を付けて 11 行に並べる。左歩きは右歩きの反転。足りない状態は近い状態の絵で埋める。`--frames <状態>=<フォルダ>` で描いたアニメーションのコマ（同じ大きさの PNG・WebP を名前の順に 8 枚まで）を渡すと、その行は静止画の代わりにそのコマで作る。全部のコマの不透明な範囲を合わせた枠で同じように切り出し（コマごとに切ると足の出し入れで中心がずれて揺れる）、キャラの高さは同じ状態の静止画に合わせる（まめドラゴンの歩き 8 コマは 448×448 で、静止画の約 3 分の 1 の解像度だった）。`PET_PREVIEW=<png>` で縮小見本も出す

| 状態の絵 | 行（ポーズ） | 動き |
|---|---|---|
| idle | 0 Idle | 1 コマ（呼吸はアプリが描く） |
| walk | 1 RunRight／2 RunLeft（反転） | 描いた 8 コマ（`--frames walk=<フォルダ>`。無ければ静止画を上下に揺らす） |
| attention | 3 Waving／6 Waiting（入力待ち） | 左右 |
| happy | 4 Jumping（クリック・完了の反応） | 跳ねる |
| confused | 5 Failed | 左右 |
| working | 7 Running（ツール実行中）／8 Review | 上下（小） |
| sleeping | 9 Sleep | 1 コマ（呼吸はアプリが描く） |
| dragged | 10 Grabbed | 左右 |

**呼吸**（2026-10-21 K 指示「息をしているように揺らしているのをもう少し滑らかに」）: 待機と睡眠の呼吸はシートに焼き込まず、アプリが描くたびに時刻から計算する（`pet.json` の `breathe`＝縦の伸びの割合。まめドラゴンは 0.025。上限 0.08）。足元を固定して縦に伸び縮みさせ、1 周は待機 3.6 秒・睡眠 5.2 秒（余弦で行き来）。伸び具合は 16 段階に丸めて描いた絵を使い回す（まめドラゴンで 1 段 0.4px 未満）。焼き込みだと 1 周 6 コマで 0.9 秒ごとに約 3px の段差だった。動かない行は 1 コマだけ書く（同じ絵を 6 コマ並べると、描いた絵の保存が 6 倍になり専有が 16MB 増えた）。実測: 呼吸の段階は 20 秒で 210 回・1 回に 1 段ずつ、CPU 0.09%、専有 24.8MB

仮キャラは `cargo run -p pet-core --example gen_assets` で再生成する。

## 5. Claude Code 連携（Phase 2・2026-10-21 実装）

中心ロジックは `pet-core::hook`（stdin の JSON の解析・要約・サニタイズ・出力）と `pet-core::ipc`（発見ファイル・送る出来事・HTTP の最小限の解析）。要求の正本は `.steering/20261021-phase2-hooks/requirements.md`、テストは `crates/pet-core/tests/{hook,ipc}.rs`。

### 5.1 登録する hook

| hook | ペットの動き | 吹き出し | 同期 |
|---|---|---|---|
| `UserPromptSubmit` | Prompt（起き上がる） | 出ている承認のカードを下げる | 非同期（`async: true`, timeout 5） |
| `PreToolUse`（`*`） | ToolStart（作業） | なし | 非同期 |
| `PermissionRequest`（`*`） | NeedsInput（手を振る） | 承認のカード | **同期**（timeout 60）。答えが来るまで待つ |
| `Notification` | `permission_prompt`・`elicitation_dialog` は NeedsInput | `elicitation_dialog` は「入力を待ってるよ」 | 非同期 |
| `Stop` | Done（1 回だけ喜ぶ） | 「できたよ」・カードを下げる | 非同期 |
| `StopFailure` | Error | 「失敗しちゃった」・カードを下げる | 非同期 |

- 登録: `pet-app.exe --install-hooks [--settings <path>]`、解除: `--uninstall-hooks`（既定は `~/.claude/settings.json`。実体 `crates/pet-app/src/install.rs`）。コマンドは exec 形式（`command` に同じフォルダの `pet-hook.exe` の絶対パス、`args: ["--pet-claude-managed"]`）。印の付いた項目を全部消してから足すので何度実行しても 1 組。印の無い項目と他のキーには触らない。書く前に `<名前>.pet-claude-backup-<時刻>` に複製し、一時ファイル＋rename で置き換える。JSON として読めないファイルは書き換えない
- 送るのはツール名・短い要約（120 字。Bash はコマンド、ファイル系はパス）・通知の種類だけ。プロンプトや応答の本文は送らない（テストで固定）
- 間引きは置かない。openpets は hook ごとに Node を起動し、出来事ごとに吹き出しを出すので間引きが要った。こちらは Rust の小さな exe（故障注入で起動から終了まで 1.5 秒未満、通常は数十 ms）で、吹き出しは応答の終わりなど 1 往復に 1 回の出来事にしか出さない

### 5.2 承認の往復

1. hook が `POST /permission` で登録して id を受け取り、`GET /permission/<id>` を 300ms ごとに最大 55 秒（`PET_PERMISSION_WAIT_SECS`）ポーリング
2. アプリは吹き出しに承認のカード（「ターミナルで」「拒否」「許可」）を出す。複数あれば古い順に 1 枚ずつ（「ほかに N 件」）
3. hook の出力: 許可 `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}`、拒否は `behavior: "deny"` と `message`（「ペット（pet-claude）の吹き出しで拒否されました」。Claude に届くことを E2E で確認）
4. 「ターミナルで」・時間切れ・アプリが居ない・失敗は `{}`（いつもの流れに戻す。`-p` では拒否扱いになることを E2E で確認）。hook は**どんな失敗でも終了コード 0**
5. 「許可」は、見えている要約が操作の全部のときだけ出す（全文の見えない操作を一押しで通させない。2026-10-21 セキュリティレビューと検証係の指摘）。条件は次の 3 つ。アプリ側の `ui::permission_decide` でも同じ条件で許可を受け付けず ask にする
   - hook が「要約が操作の全部をそのまま見せている」の印（`detail: "exact"`。`pet_core::hook::summary_is_exact`）を付けた。付くのは、要約が操作の全部になるツール（Bash・PowerShell の command、Read の file_path、WebSearch の query）で、`tool_input` に実行の中身を変えない項目（description・timeout・offset・limit）しか無く、要約の元が加工されていない（改行・タブ・空白の並び・前後の空白・切り詰めが無い）ときだけ。Agent の prompt・Write／Edit の本文・Bash の `dangerouslyDisableSandbox` などは要約に出ないので付かない
   - 要約が見た目どおりに読める文字だけ（`pet_core::hook::is_plain_char` の許可リスト: ASCII の印字可能文字・Latin-1・かな・漢字・全角英数・CJK の句読点。空白に見える文字・見えない文字・結合文字・絵文字は不可）
   - 要約を折り返して 4 行に収まる（半角 36 桁/行・全角は 2 桁の見積もり。カードは行数に合わせて高くする）。見積もりが外れてカードからはみ出したら、ページ側も「許可」を隠す
   - 出たばかりのカード（先頭になって 600ms 以内）への「許可」「拒否」は捨てる。前のカードへの二度押しの 2 回目が、読んでいない次のカードに入らないように（ページ側も同じ時間押せない）
6. 同じセッションで次の依頼・応答の終わりが来たら、残っているカードを下げ、その hook には「決めない」（ask）を返す（ターミナルで答え済み）。並行のツール呼び出しがあるので `PreToolUse` では下げない。hook が 5 秒問い合わせない（あきらめた・ターミナルで答えて終わった）・120 秒を過ぎた承認も、1 秒ごとの見回りで下げる
7. 試験用: デバッグビルドだけ `PET_AUTO_DECIDE=allow|deny|ask` で、ボタンと同じ関数を 1.5 秒後に呼ぶ（`scripts/verify/e2e-hooks.sh`）。`PET_CLAUDE_DATA_DIR` を差し替えた時だけ効く（普段使いのデバッグビルドで環境変数が残っていても、承認が自動で通らないように）

### 5.3 IPC

- アプリ起動時に `127.0.0.1:0` で待ち受け、`%LOCALAPPDATA%\pet-claude\runtime\ipc.json` に `{endpoint, token, pid}` を一時ファイル＋rename で書く。終了時に消す。`PET_CLAUDE_DATA_DIR` で置き場所を差し替えられる（試験用。アプリと hook で同じ規則）
- token は `BCryptGenRandom` の 32 バイト。取れなければ受け口を開かない（時刻などで代用すると推測される）。hook は `Authorization: Bearer <token>` を付ける。token が違う・`Origin` ヘッダが付いている（ブラウザからの要求）なら 401
- hook は、発見ファイルの pid のプロセスが生きていて、その実行ファイルのフルパスが hook と同じフォルダの `pet-app.exe` で、発見ファイルより前に生まれているときだけ繋ぐ（アプリが強制終了して残った発見ファイルのポートで、別のプロセスが待ち受けて答えを横取りするのを防ぐ。後から同じ pid を引き当てたプロセスは生成時刻で弾く）。残る穴: ポートの持ち主までは確かめていない（同じフォルダの pet-app.exe を書き換えられる相手には効かないが、その相手はもっと強いことができる）
- HTTP は自前の最小限の解析（要求は 128KB まで。`Transfer-Encoding`・重複ヘッダ・単独の CR/LF は 400）。接続ごとにスレッドを分け（同時 16 本まで）、要求全体を 2 秒で読み切れなければ切る（少しずつ送る接続 1 本で他の hook を止めさせない）
- hook は接続 300ms・読み書き 500ms、応答全体 1 秒・16KB で諦める
- 承認は 120 秒、または hook が 5 秒問い合わせなければ失効（GET は 404、答えは受け付けない）。hook の待ちは最大 55 秒・問い合わせは 300ms ごと
- 故障注入: hook 側 `crates/pet-hook/tests/fault.rs`（9 本）、アプリ側 `scripts/verify/fault-app.py`（遅い接続・期限後に完成した要求・放置の失効）

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
