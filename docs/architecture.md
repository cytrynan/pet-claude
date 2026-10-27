---
type: Architecture
description: pet-claude の技術仕様。Rust 1 プロセス。ペット本体は Win32 の layered window、吹き出し・設定などの UI は Tauri 2（WebView2）。入力（hooks）・行動・描画・声を分離する
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-21T19:50:00+09:00 }
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
| 透過・枠なし | `WS_POPUP` + `WS_EX_LAYERED`。`UpdateLayeredWindow(ULW_ALPHA)` に BGRA 事前乗算済みの DIB を渡す。DIB とメモリ DC は 1 組を使い回し、窓の大きさが変わったときだけ作り直す（2026-10-21。描くたびに作り直すと、105%・200% の画面で待機の CPU が 2% を超えた） | 合格 |
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
| バー（`bar`） | ペットをクリック、または**カーソルがペットの不透明な部分に乗って 200ms**（K 指示 2026-10-21。バー・チャット・メニューが出ている間は乗っても出さない）。ペットの下・中央。下に収まらない時（ペットが作業領域の下端にいる時）はペットの上・中央 | 取らない（`focusable(false)`） | もう一度ペットをクリック／外をクリック／メニュー「バーを隠す」／**乗って出したものは、ペットとバーの外へ出て 400ms で閉じる**（低レベルのマウスフックで見る）。乗って出たバーは、ペットのクリック・チャットを開く・右クリックで固定され、離れても閉じない |
| チャット入力（`chat`） | バーの「チャット」。バーと同じ場所 | 取る（文字を打つため。利用者が選んだとき） | **ペットを左クリックした時だけ**（K 指示 2026-10-22「一度開いたら pet を左クリックするまで閉じないようにして」）。フォーカスが外れる・外をクリック・Esc・答えが来る・ペットのドラッグでは閉じない（Esc は日本語入力の変換の取り消しにも使うため）。ペットをドラッグ・位置のリセットで動かすと、ついて行く。判定は `ui::closes_on` の 1 か所 |
| メニュー（`menu`） | ペットを右クリック。ペットの横・カーソルの高さ | 取らない | 項目を押す／外をクリック |
| 設定（`settings`） | メニュー「設定」かトレイ「設定」 | 取る（普通の窓） | 窓を閉じる（破棄） |

- フォーカスを取らない窓は Esc もフォーカスの喪失も来ないので、**バーかメニューが開いている間だけ低レベルのマウスフック（`WH_MOUSE_LL`）を入れ**、島とペットの外でボタンが押されたら閉じる（チャット欄は外を押しても閉じないので、チャット欄だけの間はフックを入れない）。閉じる処理はフックの中でせず、イベントループに回す
- 窓は島より四方 12px 大きく取り、その中に影を描く（透過窓の外に影は出せない）
- **吹き出しの固定文言の大きさは文字から決める**（`pet_core::bubble::status_island`。2026-10-27 K の報告「長い文言が切れる」）: 字幅の見積もり（全角 13.25・英大文字 10・m/w 10.5・数字 7.5・その他の半角 7.0px。Edge で 13px を実測した値より広い側）で `pre-wrap; overflow-wrap: anywhere` と同じ折り返しを数え、幅は文字の幅＋左右 16px（180〜320。320 は承認のカードと同じ）、折り返したら 320 で高さを 10＋20×行数＋10 に取る（上限なし・省略しない）。見積もりが外れて中身がはみ出したら、ページが描画後に測って `ui_action("bubble:fit:<seq>:<w>:<h>")` で伝え、窓を広げる（縮めない。承認のカードは対象外）。見た目は `render-ui.js` の `status-*` で明暗ともはみ出し 0。根拠と実測は `.steering/20261027-fix-bubble-size/brief.md`
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
- クリックの反応は `Petted`（なでられ）、完了は `Jumping`（喜ぶ）、失敗は `Failed`（2026-10-21 K の動きの一覧でクリックを分けた。§4.5）
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

- ペット＝フォルダ（`pet.json`＋スプライトシート PNG/WebP）。`pet.json` の中のパス（`spritesheetPath`・`clips`）はフォルダの中の相対パスだけ受け付ける（`..`・絶対パス・ドライブ名・UNC・`.` 始まりは読み込まない）。クリップ名は英小文字・数字・`_` だけ（2026-10-21 セキュリティレビューの指摘。フォルダの中に外を指すリンクを置かれた場合は防げない）。`pet.json` は Codex 互換の `id`・`displayName`・`spritesheetPath` に、独自の `cellScale`（1 コマを 192×208 の何倍で描いたか。既定 1・上限 4）と `wakeWord`（呼びかけの言葉。ペットごとに決める。前後の空白を除き、空・16 字超・改行や制御文字を含むものは無いものとする。設定画面のペットの下に出す。聞き取りは Phase 4。2026-10-22 K 指示）を足した。`fullCell`（動画のコマを元の大きさで持つ。無ければ 3/4。2026-10-27 K 決定で次のペットから `true`）も足した。`build_pet` で作り直しても、シートに関わらない項目（`clips`・`wakeWord`・`fullCell`）は残す
- 探す場所（先に見つかった id を使う）: `%LOCALAPPDATA%\pet-claude\pets\<id>\` → exe と同じフォルダの `pets\<id>\` → デバッグビルドだけリポジトリの `assets\pets\<id>\`。内蔵の仮キャラ（`placeholder`）は exe に埋め込む
- 入っているペット（2026-10-27 時点）: まめドラゴン（`mame-dragon`・2026-10-21）と、2 体目のりんりん（`rinrin`・パンダ・2026-10-27 K 指示。呼びかけは「りんちゃん」）。どちらも `cellScale` 2・`breathe` 0.025・動画クリップ 13 本。元素材と作り直し方は各フォルダの `SOURCE.md`
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

### 4.5 動画クリップ（2026-10-21 K 指示・まめドラゴンの 13 本。2026-10-27 りんりんも同じ 13 本）

姿勢を静止画で切り替えると大きさや向きが急に変わるので、姿勢の間を動画でつなぐ。仕様の正本は `.steering/20261021-clip-playback/`（requirements.md の「計画の規則」と design.md）。

- **素材**: `pet.json` の `clips`（例 `"clips.json"`）。無いペットは今までどおりシートだけで動く（Codex 互換のシートはそのまま）。`clips.json` は `fps` と、クリップごとの `name`・`kind`（loop・transition・once）・`from`・`to`（`["idle","left"]` の形の (姿勢, 向き)）・`frames`。コマは `clips/<name>/000.webp` から連番で、1 コマはシートと同じ大きさ（`cellScale` 倍の 192×208）の透過 WebP
- **次のペットから（2026-10-27 K 決定。りんりんの画質の試しで「960 生成＋等倍」を K が選んだ。まめドラゴン・りんりんは 640・3/4 のまま）**: 手順と道具の正本はプロジェクトスキル `.claude/skills/make-pet-clips/`（展開 → 姿勢画像 → 指示文 → workflow → Comfy Cloud → 検分 → K 判定 → コマ化 → シート → 実機 → 記録。無料枠が足りなければ K がアカウントを切り替える）。要点: ①姿勢画像は 960×960（`align_poses.py 960`。床 900・中心 480・待機の高さ 720） ②Comfy の指示 JSON は `"size": [960, 960]`（1 回の実行時間は 640 の 2.5〜3 倍の見込み。4 回目の実測: 待機 81f＋なでられ 49f で約 1 回） ③`clips-src.json` は `"canvas": 960, "poses": "poses960"` ④`pet.json` に `"fullCell": true`。効き目: 960 のコマは輪郭の半透明の帯が 3 割少なく、等倍で持つと 200% の画面で引き伸ばしが 2 倍 → 1.5 倍になる。100% の画面（窓 288×312）では差が出ない。メモリは等倍で 200% の画面 25.9MB → 40.2MB（待機のみ・27 秒時点）。散歩・眠るを足した時の N1（50MB）超えは未計測
- **作り方**: `python scripts/build_clips.py <ソースの JSON> assets/pets/<id>`。動画（白背景・16fps）の末尾 3 コマを捨て（Wan 2.2 は最後の 2〜3 コマがかすむ）、8fps に間引き、白を抜き、1 コマ目を始点の姿勢画像に合わせてから全クリップ共通の倍率でコマへ置く。まめドラゴンのソースは `.steering/20261021-clip-playback/clips-src.json`（共通の倍率 0.69。全部が収まる 0.588 にすると、横に長い眠る姿勢に引っ張られて全体が 2 割小さくなるので、眠る・起きるの尻尾の先が最大 29px 見切れるのを許した）
- **計画**（`pet-core::clips::Player`。テスト先行 59 本）: 行動のポーズから目標の (姿勢, 向き) を決め、今の見た目からそこまでのつなぎを最短で選んで順に流す（左右反転を含む）。経路が無ければ切り替える。つまみ上げだけはつなぎの途中でもすぐ切り替え、ほかはつなぎを最後まで流してから最新のポーズで計画し直す。ループの無い姿勢（眠る・呼びかけ）はつなぎの最後のコマで止める。窓を横に動かすのは歩きのループの間だけ
- **クリック**は「なでられ」（`Pose::Petted`）、完了は今までどおり「喜ぶ」（`Jumping`）。シートだけのペットは `Petted` を `Jumping` の行で描く
- **持ち方**（`pet-app::clipstore`）: 13 本 361 コマを窓の大きさで全部展開すると 250MB を超えるので、流すクリップだけを別スレッドで読み、窓の大きさに縮めて事前乗算済みの BGRA で不透明な範囲だけを持つ。持つ解像度は元のコマの 3/4（288×312）まで（`pets::max_cell`。それより大きい窓には描く瞬間に整数の双線形補間で引き伸ばす）。試験用に `PET_TEST_FULL_CELL=1`（デバッグビルドで `PET_CLAUDE_DATA_DIR` を指定した時だけ）で等倍まで持てる（2026-10-27 画質の試し。りんりん・大きさ 150%・27 秒時点の専有: 200% の画面で 3/4 25.9MB → 等倍 40.2MB、100% の画面は窓が 288×312 で上限に届かず 22.6 → 26.5MB。960×960 で生成したコマは 640 より輪郭の半透明の帯が 3 割少ない。採否は K 未決）。上限 20MB、使った順の古いものから捨てる。いま流しているクリップと、待機・つまみ中のループは捨てない（つまみ中は、読み込みを待つとつまんだ瞬間に 0.5 秒ほど直前の絵のままだった）。次に流すつなぎと、固定したループの未読のものは先に読む。読み終わるまでは直前の絵を出す（描くことで窓を動かすので、何も出さないとつまんだ窓がついて来ない）。シートは、クリップで描かないポーズの行（困る・呼びかけ・入力待ち・眠る）と、最初の 1 枚に使う待機の行だけを、同じ 3/4 までで持つ。眠るのつなぎの最後のコマで止めたときの呼吸は 8 段
- 読み込みの要求には、プロセス全体で一意な番号を付ける。ペットを A→B→A と切り替えたとき、古い A の結果を新しい A の結果として受け取らないため（検証係の指摘）
- 画素を 1 つずつ回す処理は `pet-core::pixels` に置く（pet-core はデバッグビルドでも最適化される。pet-app に置いたら待機の 39 コマの読み込みに 3〜4 秒かかった。移して 0.6 秒）
- **実測**（2026-10-21・デバッグビルド・`scripts/verify/clips-sweep.sh`）:

| 条件 | 散歩あり 専有 | 散歩なし 専有 | 待機の CPU | 散歩中の CPU |
|---|---|---|---|---|
| K の設定（大きさ 150%・100% の画面・窓 288×312） | 36.1MB | 35.9MB | 0.47% | 19.7% |
| 要件 R5（大きさ 105%・200% の画面・窓 403×437） | 40.4MB | 40.3MB | 1.56〜1.88%（3 回） | 18.9% |
| 動画なしの HEAD（K の設定） | 38.3MB | — | — | 22.3% |
| 動画なしの HEAD（R5・DIB を描くたびに作り直す版） | — | 44.5MB | 2.34% | — |

  待機の CPU は、起動直後の吹き出しが出る前の 10 秒（散歩なし）。DIB を使い回す前は R5 で 2.81%（動画なしの HEAD も 2.34%）だった。吹き出しを出している間は専有 136〜141MB（N1 の UI 表示中 150MB の内）。10 秒の計測は回ごとに 0.5pt ほど揺れる

## 5. Claude Code 連携（Phase 2・2026-10-21 実装）

中心ロジックは `pet-core::hook`（stdin の JSON の解析・要約・サニタイズ・出力）と `pet-core::ipc`（発見ファイル・送る出来事・HTTP の最小限の解析）。要求の正本は `.steering/20261021-phase2-hooks/requirements.md`、テストは `crates/pet-core/tests/{hook,ipc}.rs`。

### 5.1 登録する hook

| hook | ペットの動き | 吹き出し | 同期 |
|---|---|---|---|
| `UserPromptSubmit` | Prompt（起き上がる） | 出ている承認のカードを下げる | 非同期（`async: true`, timeout 5） |
| `PreToolUse`（`*`） | ToolStart（作業） | なし | 非同期 |
| `PreToolUse`（`AskUserQuestion`） | NeedsInput（手を振る） | 質問のカード（§5.4） | **同期**（timeout 300・`--ask`）。答えが来るまで待つ |
| `PermissionRequest`（`*`） | NeedsInput（手を振る） | 承認のカード | **同期**（timeout 60）。答えが来るまで待つ |
| `Notification` | `permission_prompt`・`elicitation_dialog` は NeedsInput | `elicitation_dialog` は「入力を待ってるよ」 | 非同期 |
| `Stop` | Done（1 回だけ喜ぶ） | 「できたよ」・カードを下げる | 非同期 |
| `StopFailure` | Error | 「失敗しちゃった」・カードを下げる | 非同期 |

- 登録: `pet-app.exe --install-hooks [--settings <path>]`、解除: `--uninstall-hooks`（既定は `~/.claude/settings.json`。実体 `crates/pet-app/src/install.rs`）。コマンドは exec 形式（`command` に同じフォルダの `pet-hook.exe` の絶対パス、`args: ["--pet-claude-managed"]`）。印の付いた項目を全部消してから足すので何度実行しても 1 組。印の無い項目と他のキーには触らない。書く前に `<名前>.pet-claude-backup-<時刻>` に複製し、一時ファイル＋rename で置き換える。JSON として読めないファイルは書き換えない
- 送るのはツール名・短い要約（120 字。Bash はコマンド、ファイル系はパス）・通知の種類だけ。プロンプトや応答の本文は送らない（テストで固定）
- 間引きは置かない。openpets は hook ごとに Node を起動し、出来事ごとに吹き出しを出すので間引きが要った。こちらは Rust の小さな exe（故障注入で起動から終了まで 1.5 秒未満、通常は数十 ms）で、吹き出しは応答の終わりなど 1 往復に 1 回の出来事にしか出さない

### 5.2 承認の往復

1. hook が `POST /permission` で登録して id を受け取り、`GET /permission/<id>` を 300ms ごとに最大 55 秒（`PET_PERMISSION_WAIT_SECS`）ポーリング
2. アプリは吹き出しに承認のカード（「ターミナルで」「拒否」「許可」）を出す。複数あれば古い順に 1 枚ずつ（「ほかに N 件」）。「ターミナルで」と注記の「〜で」は要求の来た先で変える（7）
3. hook の出力: 許可 `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}`、拒否は `behavior: "deny"` と `message`（「ペット（pet-claude）の吹き出しで拒否されました」。Claude に届くことを E2E で確認）
4. 「ターミナルで」（来た先により「Claude アプリで」「Claude Code で」）・時間切れ・アプリが居ない・失敗は `{}`（いつもの流れに戻す。`-p` では拒否扱いになることを E2E で確認）。hook は**どんな失敗でも終了コード 0**
5. 「許可」は、見えている要約が操作の全部のときだけ出す（全文の見えない操作を一押しで通させない。2026-10-21 セキュリティレビューと検証係の指摘）。条件は次の 3 つ。アプリ側の `ui::permission_decide` でも同じ条件で許可を受け付けず ask にする
   - hook が「要約が操作の全部をそのまま見せている」の印（`detail: "exact"`。`pet_core::hook::summary_is_exact`）を付けた。付くのは、要約が操作の全部になるツール（Bash・PowerShell の command、Read の file_path、WebSearch の query）で、`tool_input` に実行の中身を変えない項目（description・timeout・offset・limit）しか無く、要約の元が加工されていない（改行・タブ・空白の並び・前後の空白・切り詰めが無い）ときだけ。Agent の prompt・Write／Edit の本文・Bash の `dangerouslyDisableSandbox` などは要約に出ないので付かない
   - 要約が見た目どおりに読める文字だけ（`pet_core::hook::is_plain_char` の許可リスト: ASCII の印字可能文字・Latin-1・かな・漢字・全角英数・CJK の句読点。空白に見える文字・見えない文字・結合文字・絵文字は不可）
   - 要約を折り返して 4 行に収まる（半角 36 桁/行・全角は 2 桁の見積もり。カードは行数に合わせて高くする）。見積もりが外れてカードからはみ出したら、ページ側も「許可」を隠す
   - 出たばかりのカード（先頭になって 600ms 以内）への「許可」「拒否」は捨てる。前のカードへの二度押しの 2 回目が、読んでいない次のカードに入らないように（ページ側も同じ時間押せない）
6. 同じセッションで次の依頼・応答の終わりが来たら、残っているカードを下げ、その hook には「決めない」（ask）を返す（ターミナルで答え済み）。並行のツール呼び出しがあるので `PreToolUse` では下げない。hook が 5 秒問い合わせない（あきらめた・ターミナルで答えて終わった）・120 秒を過ぎた承認も、1 秒ごとの見回りで下げる
7. 来た先の判定（2026-10-24。`pet_core::origin`）: K は CLI とデスクトップアプリの両方を使い、アプリから来た要求に「ターミナルで」は誤り。hook（pet-hook）が環境変数から決めて `WireEvent.origin` に付ける
   - `CLAUDE_CODE_ENTRYPOINT=cli` → ターミナル（「ターミナルで」）
   - `claude-desktop`・`claude-desktop-3p`・`local-agent` で、アプリが付ける `CLAUDE_CODE_EXECPATH` と `CLAUDE_PID`（hook を起こした Claude Code）の実行ファイルが同じ → アプリ（「Claude アプリで」）。Claude Code はこの変数が既にあれば上書きしないので、アプリのセッションの中から起こした `claude` も `claude-desktop` を引き継ぐ。実行ファイルの照合でそれを弾く
   - それ以外（入れ子・`-p` の `sdk-cli`・VS Code・`origin` の無い古い pet-hook）→ 中立の「Claude Code で」。外れた場所を指すより、どこでも正しい言い方にする
   - 根拠は実測（`.steering/20261024-permission-origin/brief.md`）。値の一覧は公式の監視ドキュメントの `app.entrypoint` にあるが、`CLAUDE_CODE_ENTRYPOINT` と `CLAUDE_CODE_EXECPATH` が環境変数で渡ることは公式に書かれていない（`CLAUDE_PID` は書かれている）。Claude Code の更新で変わったら「Claude Code で」に落ちる（誤った場所は指さない）
8. 試験用: デバッグビルドだけ `PET_AUTO_DECIDE=allow|deny|ask` で、ボタンと同じ関数を 1.5 秒後に呼ぶ（`scripts/verify/e2e-hooks.sh`）。`PET_CLAUDE_DATA_DIR` を差し替えた時だけ効く（普段使いのデバッグビルドで環境変数が残っていても、承認が自動で通らないように）

### 5.3 IPC

- アプリ起動時に `127.0.0.1:0` で待ち受け、`%LOCALAPPDATA%\pet-claude\runtime\ipc.json` に `{endpoint, token, pid}` を一時ファイル＋rename で書く。終了時に消す。`PET_CLAUDE_DATA_DIR` で置き場所を差し替えられる（試験用。アプリと hook で同じ規則）
- token は `BCryptGenRandom` の 32 バイト。取れなければ受け口を開かない（時刻などで代用すると推測される）。hook は `Authorization: Bearer <token>` を付ける。token が違う・`Origin` ヘッダが付いている（ブラウザからの要求）なら 401
- hook は、発見ファイルの pid のプロセスが生きていて、その実行ファイルのフルパスが hook と同じフォルダの `pet-app.exe` で、発見ファイルより前に生まれているときだけ繋ぐ（アプリが強制終了して残った発見ファイルのポートで、別のプロセスが待ち受けて答えを横取りするのを防ぐ。後から同じ pid を引き当てたプロセスは生成時刻で弾く）。残る穴: ポートの持ち主までは確かめていない（同じフォルダの pet-app.exe を書き換えられる相手には効かないが、その相手はもっと強いことができる）
- HTTP は自前の最小限の解析（要求は 128KB まで。`Transfer-Encoding`・重複ヘッダ・単独の CR/LF は 400）。接続ごとにスレッドを分け（同時 16 本まで）、要求全体を 2 秒で読み切れなければ切る（少しずつ送る接続 1 本で他の hook を止めさせない）
- hook は接続 300ms・読み書き 500ms、応答全体 1 秒・16KB で諦める
- 承認は 120 秒、または hook が 5 秒問い合わせなければ失効（GET は 404、答えは受け付けない）。hook の待ちは最大 55 秒・問い合わせは 300ms ごと
- 故障注入: hook 側 `crates/pet-hook/tests/fault.rs`（9 本）、アプリ側 `scripts/verify/fault-app.py`（遅い接続・期限後に完成した要求・放置の失効）

### 5.4 質問（AskUserQuestion・2026-10-27 実装）

Claude Code が `AskUserQuestion` で聞いてきた質問に、ペットの吹き出しで答える（K 指示 2026-10-27。正本 `.steering/20261027-ask-question/brief.md`）。中心ロジックは `pet-core::ask`（解析・出してよいかの検査・hook の出力・1 問ずつの進み `Progress`。テストは `crates/pet-core/tests/ask.rs`）。

- **hook の仕組み**（公式 hooks リファレンス「Tools that require user interaction」）: `PreToolUse`（matcher `AskUserQuestion`）が `{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow","updatedInput":{"questions":[元のまま],"answers":{"<質問文>":"<ラベル>"}}}}` を返すと、質問の画面を出さずに進む（`allow` だけでは足りない）。複数選択はラベルを `, ` でつなぐ。**選択肢に無い自由文も受ける**（2026-10-27 CLI の対話で実測。「User answered Claude's questions: → むらさき」と出て Claude はその答えを使った）。拒否は `permissionDecision: "deny"`＋`permissionDecisionReason`（承認の拒否と同じ文言）
- **登録**: `--install-hooks` が `PreToolUse` に matcher `AskUserQuestion`・**同期**・`timeout` 300（既定の 60 秒では読んで考える時間が足りない）・`args: ["--pet-claude-managed", "--ask"]` の組を足す。既存の `*`（非同期）の組は残り、pet-hook は `--ask` の無い AskUserQuestion の `PreToolUse` では何もしない（同じ質問を二重に送らない。`crates/pet-hook/tests/fault.rs` の ask_*）
- **往復**: pet-hook が `POST /permission` に kind `ask_question` で質問（`WireEvent.questions`: 質問文・見出し・選択肢・複数選択か。元の文字列のまま）と来た先（§5.2 の 7）を送り、承認と同じ `GET /permission/<id>` を 300ms ごとに最大 290 秒（`PET_ASK_WAIT_SECS`）問い合わせる。アプリは 300 秒で失効（承認は 120 秒）。答えは応答の `answers`（`Reply` の並び: `{"chosen":[...]}`／`{"free":"..."}`）。hook は `ask_output` で質問に合うか検査してから出力する（合わなければ `{}`）
- **カード**（DESIGN.md「質問のカード」）: 1 問ずつ出す。見出し（`header`。複数なら `n/N`）→ 質問文（折り返し・200 字まで）→ 選択肢の行（ラベル＋説明 1 行）→ 「回答を書く…」（チャット欄を答えの入力に切り替える。1〜500 字・1 行）→ 「ターミナルで／Claude アプリで／Claude Code で」「拒否」（複数選択は「決定」）。待ちが複数なら「ほかに N 件」。出たばかりのカードと次の質問に進んだ直後は 600ms 押せない（承認と同じ）。全部答えてから 1 回で hook に返す
- **出さない時**: 設定「質問にペットで答える」がオフ（`state.json` の `ask`。既定オン）、または質問文・ラベルに URL／パス／コード／秘密らしい文字列（チャットの吹き出しと同じ `chat::is_unsafe_for_bubble`）か隠れた文字（ゼロ幅・双方向制御）がある、形が違う（5 問以上・5 選択肢以上・ラベルの重複・質問文の重複）→ 吹き出しに「聞きたいことがあるよ」を出し、`tts` がオンなら読み上げ、hook には「決めない」（`{}`）で Claude Code のいつもの画面に任せる。説明だけが出せない時は説明を空にしてカードは出す
- **試験**: デバッグビルドだけ `PET_AUTO_ASK=pick:last|pick:first|free:<文>|deny|ask`（`PET_CLAUDE_DATA_DIR` と一緒の時だけ）。端から端までは `scripts/verify/e2e-ask.sh`（対話の `claude` を ConPTY で動かす `scripts/verify/claude-tui.py`。`-p` ではモデルが AskUserQuestion を呼ばない）。見た目は `scripts/verify/render-ui.js` の `ask-*`・`chat-ask`
- **対象外**: ペットのチャット（`claude -p`）からの質問。デスクトップ版は仕組みが同じ（同じ hook・同じ環境変数）だが実機の確認は K 待ち

## 6. 声（Phase 3〜4）

| 部品 | 実装 | 無いとき |
|---|---|---|
| TTS | AivisSpeech `POST /audio_query` → `POST /synthesis`（`127.0.0.1:10101`、天深シノ・ノーマル。ai-news の `dev\ai-news-routine\lib\radio.mjs` と同じエンジンと話者）。§6.2 | 起こせない・応答しない・壊れた音は黙る |
| STT | faster-whisper small（CPU・int8）を録音ごとに子プロセスで起こす（`scripts/stt/transcribe.py`）。§6.3 | 設定画面で `scripts\setup-stt.ps1` を案内 |
| ウェイクワード | Python の子（`scripts/wake/listen.py`。声の環境の numpy・onnxruntime で動かし、openwakeword のパッケージは入れない）。openWakeWord の特徴抽出（`melspectrogram.onnx`→`embedding_model.onnx`、80ms ごとに 96 次元）の上に、ペットごとの小さな頭（直近 1.28 秒を見る 1536→32→1）を載せる。頭は AivisSpeech の合成（`synth.py`）と K の録音（`record-wake.ps1`）から `train.py` で学習し、しきい値は `evaluate.py` が誤反応の上限から決める。モデル・録音・頭は `%LOCALAPPDATA%\pet-claude\wake\` に置き、リポジトリに入れない。特徴抽出と公開の否定データは CC BY-NC-SA 4.0（非営利の個人利用）。学習の道具まで作って保留（K 2026-10-26・PRD F20）。道具（`scripts/wake/`）は git の外に置く（`.gitignore`。K 2026-10-26）。中身は commit 25f7b23・f2bac67 の履歴に残る。再開したら、マイクには声の PR #25 の後につなぐ（詳細は `.steering/20261023-phase4/design.md` §2） | 既定 OFF。頭が無い・子が落ちても pet-app は動き続ける（W5） |
| チャット | `claude -p` を送信ごとに子プロセスで起こす（§6.1。PRD U1 決定 2026-10-21） | 見つからない・未ログイン・時間切れは吹き出しに短い理由 |

話す文言は固定文言＋検証済みの短文のみ（1〜140 字・単一行・コード/URL/パス/秘密らしい文字列は拒否）。

### 6.1 チャット（Phase 3 前半・2026-10-21 実装）

バーのチャットに打った文を、ペット専用の新しいセッションに送る（ツールなし・会話だけ）。設計と実測の正本は `.steering/20261021-phase3-chat/`（design.md・evidence.md）。

- **起こし方**: 送るたびに `claude -p` を 1 回起こして終わる（Agent SDK と同じ仕組みを、常駐なしで使う）。引数は `pet_core::chat::child_args`: `--output-format json --safe-mode --model sonnet --tools "" --disallowedTools "mcp__*" --strict-mcp-config --settings {"disableAllHooks":true} --disable-slash-commands --permission-prompts none --max-turns 1 --system-prompt-file <ペットの話し方>`、続きは `--resume <session_id>`。本文は標準入力で渡す
  - `--safe-mode`: K の `~/.claude/CLAUDE.md`・自動メモリ・スキル・hooks・MCP を読まない（無いと個人の指示書が毎回入り、入力 19,156 トークン。あると 787）。pet の hooks もチャット自身の会話では鳴らない
  - `--model sonnet`: CLI 既定（Fable）は上限に達すると全く答えない。haiku でも秒数は変わらない（起動が約 3 秒を占める）。答えまで 3.4〜9.8 秒（実測）
- **実行ファイル**: `pet_core::chat::find_claude`。PATH の `claude.exe` → npm 版は `claude.cmd` の隣の `node_modules\@anthropic-ai\claude-code\bin\claude.exe`（`.cmd` は cmd.exe の引用符解釈を受けるので直接起こさない）→ `%USERPROFILE%\.local\bin\claude.exe`
- **認証**: 設定の「接続」で切り替える（既定サブスク）。子の環境は `pet_core::chat::child_env` で組み、`ANTHROPIC_API_KEY`・`ANTHROPIC_AUTH_TOKEN`・`CLAUDE_CODE_USE_BEDROCK/VERTEX/FOUNDRY`・`ANTHROPIC_PROFILE`・`CLAUDECODE`・`CLAUDE_CODE_ENTRYPOINT` を必ず消す（大文字小文字を無視。`claude -p` は環境の API キーを必ず使うため、消さないと気づかずに API 課金になる）。API キーのときだけ、資格情報マネージャー（GENERIC・`pet-claude/anthropic-api-key`。`crates/pet-app/src/cred.rs`）から読んだ鍵を 1 つ足す。鍵は画面に読み返さない・ログに書かない
- **アプリ側**（`crates/pet-app/src/chat.rs`）: 送信は待たずに戻り、別スレッドで子を起こす（`CREATE_NO_WINDOW`）。本文は `input_text` で 2000 字まで（超えたら断る）。同時に 1 本だけ（受け付けの時点で予約し、答えを受けて `session_id` を覚えてから外す＝メインスレッドが詰まって手順がまとめて走っても 2 本起きず、続きも切れない）。子は**止めた状態で**（`CREATE_SUSPENDED`）起こして送信ごとの Job Object（閉じると中身ごと止まる）に入れてから動かす（入れる前に孫が逃げないように。入れられなければ動かさない）。本文は別スレッドで標準入力に書く（子が読まずにパイプが詰まっても時間切れが効く）。120 秒で時間切れ。「新しいチャット」・終了で止め、世代番号で古い答えを捨てる。pet-app が落ちても OS がハンドルを閉じて子も止まる。待つ間ペットは作業中の動き（行動ロジックには `pet-chat` のセッションとして渡す）
- **表示**: 答えは `pet_core::chat::bubble_text`（N3。検査は切り詰める前の 1 行で行い、URL・パス・コード・秘密らしい文字列があれば「（答えはチャット欄に出しました）」）を吹き出しに、`panel_text`（秘密らしい文字列を `••••` に伏せ、改行を残して 4000 字まで）をチャット欄に。吹き出しに全部出ていなければ「続きを見る」でチャット欄を開く。失敗の理由は `parse_output` が秘密を伏せた 1 行（200 字まで）で、チャット欄にはそのまま（パスは見える）、吹き出しは `bubble_text` に掛かれば「うまくいきませんでした。理由はチャット欄に」＋「続きを見る」
- **重なり**: チャット欄はバーと同じ規則で置く（ペットの真下、作業領域の下に収まらなければ真上。`ui::below_pet`）。答えの欄が足されて高くなった時も同じ規則で置き直すので、ペットには重ならない（K 指示 2026-10-22「端っこでチャットを開くと重なる問題も直して」）。念のため、チャット欄が出ている間はペットは最前面を取り直さない。開いた直後 600ms 以内に前面を失っても 1 回だけ取り直す（窓を作った直後は 7ms で失って閉じていた）
- **「確かめる」**: `claude auth status` もチャットと同じ起こし方（Job・出力の上限）で、15 秒で時間切れ
- **ログ**: 字数・秒数・終了コード・is_error・接続の種類だけ。本文・答え・鍵は書かない（N4）
- **作業フォルダ**: `%LOCALAPPDATA%\pet-claude\chat\work`（空。リポジトリや K の作業フォルダの設定を拾わない）。ペットの話し方は同じフォルダの上の `system-prompt.txt` に毎回書き出す（中身はコードの定数）
- **検証**: 単体 `crates/pet-core/tests/chat.rs`（テスト先行 97 本）、故障注入 `scripts/verify/chat-fault.py`（偽の claude `examples/fake_claude.rs` で見つからない・is_error・壊れた出力・時間切れ・新しいチャット・アプリが落ちる・1 本だけ・まとめて来た手順・続き・環境・鍵が無い。18 項目（panic の故障注入を含む）。取り消し・まとめて来た手順は、アプリを止める前に子が残っていないかを見る）、見た目 `scripts/verify/render-ui.js`（invoke の偽物＋Edge で設定・チャット欄・吹き出しを撮る）。試験用の差し替え（`PET_TEST_CHAT`・`PET_CLAUDE_EXE`・`PET_CHAT_TIMEOUT_SECS`、別名のミューテックス）はデバッグビルドで `PET_CLAUDE_DATA_DIR` を指定した時だけ効く

### 6.2 読み上げ（Phase 3 後半・2026-10-22 実装）

設計と実測の正本は `.steering/20261022-phase3-voice/`。

- **話す順**: `pet_core::voice::SpeechQueue`（テスト先行）。固定文言は話し中なら捨てる・同じ文言は 15 秒以内に繰り返さない・答えは 1 つだけ待たせる（新しい答えが古い方を押しのける）。話す文は `speech_for_answer`＝吹き出しと同じ `bubble_text` を通した 1 行（N3）。吹き出しに出せない答えは「答えはチャット欄に出したよ」
- **アプリ側**（`crates/pet-app/src/tts.rs`）: 専用スレッド 1 本。HTTP は `TcpStream` で手書き（新しい crate を足さない。chunked も読む）。style id は `/speakers` から名前で引く（先頭のスタイルに逃げない）。音は `parse_wav` で頭を確かめてから `PlaySoundW(SND_MEMORY|SND_SYNC)`。「声で知らせる」をオフにすると `PlaySoundW(NULL)` で止める
- **エンジンの起動**: `/version` が返らなければ `%USERPROFILE%\dev\voice\_engines\AivisSpeech\AivisSpeech-Engine\run.exe` を窓なしで起こし、閉じると止まる Job に入れる（pet-app が落ちても残らない）。最大 90 秒待つ。起こすのに 5 秒以上かかったら、待たせていた固定文言は捨てる。ペットが起こした時だけ、最後に話してから 10 分で止める。既に動いていたエンジンには触らない
- **呼ぶ所**: チャットの答え・失敗（「うまくいかなかったよ」）、hooks の固定文言（できたよ・失敗しちゃった・入力を待ってるよ）、承認のカード（「確認してほしいことがあるよ」）
- **話し出すまでを縮める**（2026-10-22 K「話し出すまでの時間を短縮することはできないの？」）: 答えは `speech_chunks` で文ごとに分け（6 字未満の文は隣とまとめる）、1 文目ができたら鳴らし、鳴らしている間に次の文を別スレッドで合成する。音声入力の録音を始めた時とチャット欄を開いた時に `prewarm` でエンジンを起こし、起こした直後は短い文を 1 回合成して声のモデルを読ませる（鳴らさない）。先に起こした時も「最後に動いた時刻」を進め、話す前に暇として止めない。つなぐのは 0.5 秒で諦める（Windows は閉じた 127.0.0.1 のポートでも約 2 秒やり直す）
- **実測**（2026-10-22）: 4 字 0.36 秒、48 字 2.2 秒。エンジンは約 3.4GB。起こしてから使えるまで約 21 秒、起こした直後の 1 回目の合成はモデルの読み込みで約 5 秒

### 6.3 音声入力（Phase 3 後半・2026-10-22 実装）

- **録音**（`crates/pet-app/src/voice.rs`）: winmm の `waveIn*` で 16kHz・モノラル・16bit を 30ms ずつ 8 枚のバッファで受ける。区切りは `pet_core::voice::Endpointer`（最初の 300ms の最小音量の 3 倍か 300 の大きい方を超えたら話している。話して 300ms 以上の後に 1 秒黙れば止める・5 秒話さなければ取りやめ・最長 15 秒）。もう一度押すと止めて送る。全部 0 の録音は Windows のプライバシー設定で止められている形として「マイクを使えないよ」
- **文字起こし**: 子（`%LOCALAPPDATA%\pet-claude\stt\venv\Scripts\python.exe -I %LOCALAPPDATA%\pet-claude\stt\transcribe.py`。スクリプトは `scripts/stt/transcribe.py` を exe に埋め込み、使う時にデータフォルダへ書き出す＝ビルドしたリポジトリのパスを実行しない）は**録音を始める前に**起こし、K が話している間に Python の起動とモデルの読み込みを済ませる。録り終えたら `%LOCALAPPDATA%\pet-claude\voice\rec-<世代>.wav` に書き、そのパスを標準入力で渡す（`child.rs` の `Input::Later`。取りやめならパスを渡さずに閉じ、子は何もせず終わる）。チャットと同じ起こし方・90 秒で時間切れ（録音とモデルの読み込みを含む）・`HF_HUB_OFFLINE=1`。WAV は成否に関係なく消し、起動時にも残りを消す。出力は ASCII の JSON 1 行（`-I` では `PYTHONIOENCODING` が効かず、パイプが CP932 になるため）
- **送る**: `transcript_text`（幻聴の決まり文句・句読点だけ・空は捨てる）を通った文をチャット欄に K の発言として出してから `chat::submit`。チャットの返答待ちの間は録音を始めない
- **ミュート**: 録る前に既定の録音デバイスを `IAudioEndpointVolume::GetMute` で見て、ミュートなら「マイクがミュートになっているよ」。録った後も最大振幅 8 以下（`looks_muted`）はミュートか Windows の設定と見なす（ミュートのマイクは完全な 0 でなく最大振幅 1 を返した。2026-10-22 K の実機）
- **見た目**: 録音中はバーの音声ボタンが赤く脈打ち（`prefers-reduced-motion` では止まる）、バーは固定される。ペットは承認待ちと同じこちらを見る動き、文字起こし中は作業中の動き
- **準備**: `scripts/setup-stt.ps1`（venv を作り `faster-whisper==1.2.1` を入れ、small を用意して無音で 1 回試す）
- **実測**（2026-10-22・Ryzen 7 8845HS・CUDA なし）: 6 秒の発話で small が読み込み 1.7 秒＋認識 1.6 秒。large-v3 は 7.5＋7.4 秒で、どちらも「まめちゃん」を「もめちゃん」と聞いた
- **話し終えてから声が出るまで**（本物の文字起こし・Claude・AivisSpeech、合成した 7.3 秒の発話。2026-10-22）:

  | | 文字起こし | Claude の答え | 1 文目の合成 | 合計 |
  |---|---|---|---|---|
  | 縮める前（エンジンは動いている） | 6.3 秒 | 6.5 秒 | 3.5 秒（全文） | 約 16 秒 |
  | 縮めた後（エンジンは動いている） | 2.1 秒 | 4.3 秒 | 2.2 秒 | 約 8.6 秒 |
  | 縮める前（エンジンが止まっている） | 9.8 秒 | 6.1 秒 | 起こす 22 秒＋5.5 秒 | 約 46 秒 |
  | 縮めた後（エンジンが止まっている） | 2.6 秒 | 6.2 秒 | 起こす待ち＋5.0 秒 | 約 21 秒 |

  残りの大半は Claude（`claude -p` の起動が約 3 秒）と、止まっていたエンジンを起こす約 21 秒
- **検証**: 単体 `crates/pet-core/tests/voice.rs`（テスト先行 93 本）、故障注入 `scripts/verify/voice-fault.py`（偽のエンジン・偽の文字起こし `fake_stt.py`・マイクの代わりの WAV。21 項目）。試験用の差し替え（`PET_TEST_AIVIS_ADDR`・`PET_TEST_AIVIS_EXE`・`PET_TEST_STT_EXE`・`PET_TEST_STT_SCRIPT`・`PET_TEST_VOICE_WAV`・`PET_STT_TIMEOUT_SECS`）はデバッグビルドで `PET_CLAUDE_DATA_DIR` を指定した時だけ効く

## 7. 設定・ログ・保存先

- `%LOCALAPPDATA%\pet-claude\state.json`: `scale`（0.25〜1.5、既定 0.55）、`positions`、`chat_auth`（`subscription` / `api_key`。無い・知らない値はサブスク）。書き込みは一時ファイル＋rename
- チャットの API キー: Windows の資格情報マネージャー（自分のユーザーだけが読める。§6.1）。`state.json` には書かない
- `%LOCALAPPDATA%\pet-claude\chat\`: チャットの作業フォルダと話し方の指示（§6.1）
- `%LOCALAPPDATA%\pet-claude\logs\pet.log`: 起動・配置・クリック・ドラッグ・DPI 変化。`PET_TRACE=1` でドラッグ中の座標も書く
- 自動起動（Phase 4・2026-10-23 実装）: 設定の「サインインしたらペットを出す」で Windows タスク `pet-claude` を登録・解除する。実物はタスク スケジューラで、`state.json` には持たない（設定画面は開くたびに読む）。中身は `pet_core::autostart::task_xml`（`%SystemRoot%\System32\wscript.exe //B //Nologo "%USERPROFILE%\.claude\scripts\run-hidden.vbs" pet-claude "<いまの pet-app.exe>"`、このユーザーのサインインから 10 秒後・時間の上限なし（既定の 72 時間で止まらないように）・電池でも動く・優先度 4（既定 7 は通常未満）・二重に起こさない）。登録は `crates/pet-app/src/autostart.rs` が `schtasks /Create /XML`（UTF-16LE＋BOM の一時ファイル）で行う。`run-hidden.vbs` が無ければ登録しない。CLI: `--install-autostart [--task-name <名前>] [--disabled]`・`--uninstall-autostart`・`--autostart-status`（試験は無効のタスクを別名で作ってすぐ消す）。記録 `.steering/20261023-phase4/`
- 端末の窓を出さない（2026-10-26 K 指示）: `pet-app` はデバッグビルドでも GUI サブシステム（`#![windows_subsystem = "windows"]`）。`--probe` などの CLI の引数の時だけ、`crates/pet-app/src/console.rs` が親の端末に `AttachConsole` でつなぎ直して出力する。標準出力が既にある（`> file`・パイプ）時はつながない（つなぐとリダイレクト先に届かない）。端末から CLI を起こすとプロンプトが先に戻ってから出力が出る（GUI のプロセスをシェルが待たないため）。`pet-hook` は hook から標準入出力のパイプで起こされるので変えていない

## 8. 検証

- `bash scripts/gate.sh`: `cargo fmt --check` → `cargo clippy -D warnings` → `cargo test`。CI も同じ内容を `windows-latest` で回す
- 実機の窓まわりは `crates/pet-app/examples/winctl.rs` で再実行できる（窓の位置・クリック・ドラッグ・撮影・画面変化の通知）。手順と結果は `.steering/20261021-phase0-window/`
- ⚠️ ゲートは clippy とテストだけで、実行ファイルを作り直さない。実機で確かめる前に `cargo build -p pet-app --all-targets` を走らせる（Phase 0 で、古い exe のまま測りかけた）
