---
type: Steering
description: Claude Code の質問（AskUserQuestion）にペットの吹き出しで答える機能の要求・受け入れ基準・非スコープ（T2）
generated: { by: kuroko/claude-fable-5-1, at: 2026-10-27T02:30:00+09:00 }
---

# 質問（AskUserQuestion）にペットで答える（T2 brief）

書類の段: **T2**（brief＋受け入れ基準・非スコープ。hooks の登録＝`~/.claude/settings.json` に触れるが、既存の `--install-hooks` の枠内で、課金・外部ユーザー・永続スキーマには触れない）。

## 要求（K 2026-10-27。司令塔のセッションから委任）

- Claude Code が質問（`AskUserQuestion`）をした時、ペットの吹き出しで「選択肢を選ぶ」「拒否」「回答（自由入力）」ができる。デスクトップ版・CLI の両方
- 既定はオン。オフの時は、ペットが「聞きたいことがあるよ」と表示し、読み上げる（読み上げがオフなら表示だけ）。質問自体はいつもの画面に任せる

## 根拠（司令塔が確かめた・2026-10-27）

- 公式 hooks リファレンス（https://code.claude.com/docs/en/hooks「PreToolUse input › AskUserQuestion」「Tools that require user interaction」）: `PreToolUse`（matcher `AskUserQuestion`）が `{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow","updatedInput":{"questions":[元のまま],"answers":{"<質問文>":"<選んだラベル>"}}}}` を返すと、聞かずに進む。複数選択はラベルをカンマでつなぐ。`allow` だけでは足りない（`updatedInput` が必須）
- 実測（Claude Code 2.1.305・CLI の対話）: hook が最後の選択肢を返したら質問の画面は出ず「User answered Claude's questions: → Blue」と出て、Claude はその答えを使った。hook に来た入力の実物は `C:\Users\meer0\AppData\Local\Temp\claude\askq-hook-input.jsonl`（`tool_input.questions[]` に `question`・`header`・`options[{label,description}]`・`multiSelect`）
- `claude -p` ではモデルが AskUserQuestion を呼ばなかった。ペットのチャット（`-p`）は対象外

## 作り（既存の承認のカードに倣う）

| 層 | 変更 |
|---|---|
| `pet-core::ask`（新規・テスト先行） | `PreToolUse` で `tool_name == "AskUserQuestion"` の時だけ `questions` を解析する型（`AskRequest`・`Question`・`Choice`）、吹き出しに出してよいかの検査（承認の要約と同じ 1 行化＋チャットの吹き出しと同じ URL/パス/コード/秘密らしい文字列の拒否）、答え（`Reply::Chosen`／`Reply::Free`・拒否・任せる）から hook の出力 JSON を組む `ask_output`、1 問ずつ答えを集める `Progress` |
| `pet-core::hook` | `HookInput::AskQuestion`（PreToolUse＋AskUserQuestion）。他のツールは今までどおり `ToolStart` |
| `pet-core::ipc` | `WireEvent.questions`（省略可）・`Reply` の wire 表現（`{"chosen":[...]}`／`{"free":"..."}`） |
| `pet-hook` | `--ask` 引数の時だけ `AskQuestion` をアプリへ送り（`POST /permission`・kind `ask_question`）、承認と同じ経路で答えを待つ（最大 290 秒）。答えが無い・アプリが居ない・設定がオフ・`--ask` 無し → `{}`（いつもの画面へ） |
| `pet-app::ipc_server` | 承認の表に質問も入れる（期限 300 秒）。`GET /permission/<id>` の応答に `answers` |
| `pet-app::ui` | 質問のカード（1 問ずつ。見出し＋質問文＋選択肢のボタン＋「ほかに N 問」「ターミナルで／Claude アプリで／Claude Code で」「拒否」「回答」。複数選択は選んでから「決定」）。「回答」は既存のチャット欄を答えの入力に切り替える。待ちが複数なら承認と同じ「ほかに N 件」 |
| 設定 | 「質問にペットで答える」スイッチ（既定オン・`state.json` の `ask`）。オフの時は「聞きたいことがあるよ」を表示し、`tts` がオンなら読み上げる |
| `--install-hooks` | `PreToolUse` に matcher `AskUserQuestion`・同期・`timeout` 300・`args: ["--pet-claude-managed", "--ask"]` を追加登録。既存の `*`（非同期）は残し、pet-hook は `--ask` が無い AskUserQuestion の `PreToolUse` では何もしない（二重に送らない） |

## 受け入れ基準

| # | 条件 | 確かめ方 |
|---|---|---|
| A1 | 選択肢を 1 つ選ぶと、質問の画面を出さずに Claude がその答えで進む（`User answered Claude's questions`） | CLI の対話（ConPTY で自動操作・`PET_AUTO_ASK=pick:last`）＋単体（`ask_output`） |
| A2 | 複数選択（`multiSelect`）はラベルを `, ` でつないで返す | 単体 |
| A3 | 「回答」の自由文を `answers` に入れて返す。Claude Code が受けるか（未確認②）を実測する | CLI の対話（`PET_AUTO_ASK=free:...`）。受けなければ報告して「回答」の扱いを K に聞く |
| A4 | 「拒否」で `permissionDecision: deny` を返し、Claude に理由が届く | CLI の対話（`PET_AUTO_ASK=deny`）＋単体 |
| A5 | 「ターミナルで」・時間切れ・アプリが居ない・`--ask` 無し → `{}`（いつもの画面が出る） | CLI の対話（`PET_AUTO_ASK=ask`）＋故障注入（pet-hook） |
| A6 | 質問が複数（2〜4 問）でも 1 問ずつ答えて、全部そろってから 1 回で返す | 単体（`Progress`）＋画面（render-ui.js） |
| A7 | 設定オフ: カードを出さず「聞きたいことがあるよ」（tts オンなら読み上げ）。hook は `{}` | 単体＋試験の置き場所でオフにして CLI の対話 |
| A8 | 質問文・選択肢に URL/パス/コード/秘密らしい文字列・見えない文字があればカードを出さず、いつもの画面へ | 単体 |
| A9 | カードは明暗で左右・上下のはみ出し 0（最長: 4 選択肢＋説明＋ほかに N 件＋Claude アプリで） | render-ui.js |
| A10 | `--install-hooks` を何度実行しても 1 組。既存の `*` 登録と AskUserQuestion の登録が両方ある | 単体（install.rs） |
| A11 | デスクトップ版でも同じに動く（未確認①） | K の実機（手順は報告に。pet-hook と常駐のアプリを両方作り直してから） |
| A12 | 品質ゲート（fmt・clippy -D warnings・test・check-ui-js・lone CR）が通る | `bash scripts/gate.sh` |

## 非スコープ

- `claude -p`（ペットのチャット）からの質問（モデルが AskUserQuestion を呼ばない）
- 質問の画面をペット側で作り直すこと（選択肢の説明は 1 行に省略。全文は「ターミナルで」で Claude Code の画面へ）
- ウェイクワード・声での回答（音声入力は既存のチャット欄の経路に乗るだけ）
- K の `~/.claude/settings.json` の書き換え・常駐のペットの差し替え（K が `--install-hooks` を実行する）

## 守ること

- ビルドは `CARGO_TARGET_DIR=target-ask`。K の常駐（`pet-claude-main\target\debug\pet-app.exe`）は止めない・上書きしない
- 実機の確認は `tmp/`（gitignore 済み）の試験用プロジェクトに `.claude/settings.json` を置き、`PET_CLAUDE_DATA_DIR` で置き場所を切り替えたペットで行う
- commit は `bash scripts/gate-commit.sh` 経由

## 実測（2026-10-27・この作業の中で）

- **未確認②（自由文）: 受ける**。`tmp/askq-free` の node の hook が `answers` に選択肢に無い「むらさき（自由回答のテスト）」を入れて返したら、CLI の対話（Claude Code 2.1.305・Sonnet）は質問の画面を出さず「User answered Claude's questions: ⎿ 好きな色はどれですか？ → むらさき（自由回答のテスト）」と出し、Claude は `ANSWER=むらさき（自由回答のテスト）` と答えた。記録 `tmp/askq-free/run1.txt`（gitignore）。対話は `scripts/verify/claude-tui.py`（ConPTY）で自動操作した
- 対話の `claude` を Claude Code の中から起こすと親の `CLAUDE*` の環境変数を引き継ぎ、「Transcript saving is off — inherited CLAUDE_CODE_CHILD_SESSION marker」になる。道具は `CLAUDE` で始まる変数を全部外して起こす

## K にしてもらうこと（デスクトップ版・本物のクリックの確認。未確認①）

前提: pet-hook とアプリは両方この枝のビルドに切り替える（K の hooks は `pet-claude-main` の pet-hook を指し、常駐も main のビルド。両方が新しくないと質問のカードは出ない）。

1. この枝を main にマージし、`pet-claude-main` で `cargo build -p pet-app --all-targets` → 常駐のペットをトレイの「終了」で止めて `target\debug\pet-app.exe` を起動し直す
2. PowerShell で `C:\Users\meer0\dev\pet-claude\pet-claude-main\target\debug\pet-app.exe --install-hooks` を 1 回（`~/.claude/settings.json` に AskUserQuestion の同期 hook が足される。元のファイルは `.pet-claude-backup-<時刻>` に残る）
3. **CLI（本物のクリック）**: ターミナルで `claude` を起こし「AskUserQuestion で好きな色を Red/Green/Blue から聞いて」と打つ → ペットの上に「好きな色」のカードが出る → **Blue を 1 回クリック** → ターミナルに「User answered Claude's questions: → Blue」と出て、質問の画面は出ない
4. 同じ指示をもう 1 回 → カードの「**回答を書く…**」をクリック → チャット欄が「質問」になり入力欄が「答えを打って送る」 → `むらさき` と打って送信 → ターミナルに「→ むらさき」
5. 同じ指示をもう 1 回 → 「**拒否**」をクリック → Claude が「拒否された（pet-claude）」と受け取る
6. 同じ指示をもう 1 回 → 「**ターミナルで**」をクリック → ターミナルにいつもの質問の画面が出る（↑↓ と Enter で答えられる）
7. **デスクトップ版**: Claude デスクトップの Code タブで同じ指示 → カードのボタンが「Claude アプリで」になっている → Blue をクリック → アプリ側の会話に「→ Blue」と出る。もう 1 回「Claude アプリで」を押すと、アプリにいつもの質問の UI が出る
8. 設定（右クリック › 設定）の「Claude Code › 質問にペットで答える」をオフ → 同じ指示 → カードは出ず、吹き出しに「聞きたいことがあるよ」（声がオンなら読み上げ）、質問はいつもの画面に出る

どれかが違ったら、`%LOCALAPPDATA%\pet-claude\logs\pet.log` の `ask requested` / `ask … state=` の行と、その時の画面をそのまま伝えてもらえれば追える。

## 受け入れ基準の判定（2026-10-27・司令塔の実測。commit 前）

| # | 判定 | 証拠 |
|---|---|---|
| A1 選択肢を選ぶ | PASS | `e2e-ask.sh` pick:last: 画面「User answered Claude's questions → Blue」「● ANSWER=Blue」・pet.log `state=allow`。単体 `chosen_answer_output_matches_official_shape` |
| A2 複数選択 `, ` | PASS（単体） | `multi_select_joins_labels_and_free_text_passes_through`・Progress の `progress_multi_select_toggles_then_submits`。CLI では複数選択の質問をモデルに作らせていない（未測） |
| A3 自由文 | PASS | e2e free:むらさき: 「→ むらさき」「● ANSWER=むらさき」・`state=allow`。未確認②は「受ける」で決着 |
| A4 拒否 | PASS | e2e deny: 「● ANSWER=DECLINED」・`state=deny`（Claude に理由が届いて DECLINED と答えた） |
| A5 `{}` でいつもの画面 | PASS | e2e ask: 質問の UI「Enter to select」が出た・`state=ask`。故障注入: アプリ不在・`--ask` 無し・時間切れ・壊れた答え（pet-hook tests 6 本） |
| A6 2〜4 問を 1 問ずつ | PASS（単体・画面） | `progress_single_choice_questions` ほか・render-ui の ask-long（2/3）。CLI で複数の質問は未測 |
| A7 設定オフ | PASS | e2e off: pet.log `enabled=false`・カード無し・質問の UI が出た。読み上げは tts オフで試験（声の経路は既存の `tts::phrase`） |
| A8 出せない質問 | PASS（単体） | `unsafe_question_is_not_presentable`・`hidden_chars_are_not_presentable`・`duplicate_labels_or_questions_are_not_presentable` |
| A9 明暗ではみ出し 0 | PASS | render-ui.js ask-single/ask-multi-more/ask-long/ask-pick/chat-ask の overflow [0]・wide [0]（明暗）・act PASS |
| A10 `--install-hooks` 1 組 | PASS（単体） | install.rs `install_twice_keeps_one_set`（PreToolUse 2 組・AskUserQuestion は timeout 300・`--ask`） |
| A11 デスクトップ版 | 未測 | K の実機（上の手順 7） |
| A12 ゲート | PASS | `bash scripts/gate.sh` GATE OK（pet-core 452 本・pet-app 35 本・pet-hook 17 本） |

本物のマウスでカードを押す確認（CLI・デスクトップとも）は未測。E2E は「ボタンと同じ関数」（`PET_AUTO_ASK`）で押している。

止まった点:
1. E2E の作業フォルダを相対パスで渡したため、hook（cwd は試験プロジェクト）が `PET_CLAUDE_DATA_DIR` を別の場所に解決してアプリを見つけられず、出来事が 1 つも届かなかった。絶対パスにして直した
2. TUI の照合 `ANSWER=` がプロンプトの echo に当たり、Claude が答える前に /exit を送っていた（3 モードが「要求 0」で FAIL）。Claude の答えの行の印「● 」で照合するようにした
3. 質問の hook が `{}` を返した後、auto mode は AskUserQuestion の PermissionRequest を起こし、ペットに要約の無い承認のカードが出た。pet-hook はそのツールの承認を送らず `{}` で任せるようにした（故障注入 1 本）
4. `wmic` が Windows 11 に無く、試験のアプリが残った。PowerShell の Get-Process で実行ファイルの場所から止める

## K の実機の結果（2026-10-27 03:43〜03:52・main 2d0e1d7 のビルド・本物のクリック）

| 手順 | 判定 | 証拠 |
|---|---|---|
| 5 Blue をクリック（CLI） | PASS | 端末「User answered Claude's questions: → Blue」「ANSWER=Blue」・pet.log action=pick → state=allow |
| 6 回答を書く…→ むらさき（CLI） | PASS | 端末「→ むらさき」「ANSWER=むらさき」・pet.log action=text → chat shown → action=answer → state=allow |
| 7 拒否（CLI） | PASS | 端末「ANSWER=DECLINED」＋「hook error: ペット（pet-claude）の吹き出しで拒否されました」・state=deny |
| 8 ターミナルで（CLI） | PASS | 端末にいつもの質問の画面（Red/Green/Blue・Enter to select）・state=ask。K は別の端末を見ていて最初「出ない」と報告したが、起こした端末タブには出ていた |
| 9 デスクトップ版（Code タブのセッションが AskUserQuestion を呼ぶ） | PASS | ボタンは「Claude アプリで」（K の目視）。ペットのカードの Blue で Claude 側に Blue が届き、アプリの質問 UI は出なかった。pet.log 03:49:53 ask requested → action=pick → state=allow |
| 10 設定オフ | PASS | pet.log `enabled=false`・カード無し・質問はアプリの画面に出て K が Red を選んだ。state.json `"ask": false`。吹き出しの「聞きたいことがあるよ」は K の目視（下に追記） |

A11（デスクトップ版）は PASS に更新。

吹き出しの「聞きたいことがあるよ」: 出た（K の目視・2026-10-27）。10 手順すべて合格。
