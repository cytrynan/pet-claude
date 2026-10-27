---
type: Steering
description: ペットのチャットを Claude Code の対話セッションと同じ環境で起こし、既定の model / effort / 権限モードを設定画面で、送信ごとの切替をチャット欄の GUI で行う（T1 brief）
generated: { by: kuroko/claude-fable-5-1, at: 2026-10-28T01:30:00+09:00 }
---

# チャットを Claude Code と同じ環境で（T1 brief）

書類の段: **T1**（brief 1 枚＋tasklist）。`state.json` にキーを 4 つ足すが、無い・知らない値は既定に倒れる後方互換の追加で、課金・外部ユーザー・認証の形は変えない。

## 要求（K 2026-10-27。司令塔のセッションから委任）

- サブスクを使う場合、ペットのチャットを **Claude Code と全く同じ環境**にしたい。既定は**ローカル・この PC**
- **model と effort とモード（権限モード）の既定は設定画面で**、**切り替えは Claude デスクトップアプリの入力欄と同じ GUI で**（入力欄の下の行。左「自動」→ モード 5 行、右「Fable 5.1 高」→ モデル 4 行＋「エフォート 高 ›」）

## 根拠（この作業で実測。Claude Code 2.1.305・サブスク max・Windows 11）

| # | 確かめたこと | 結果 |
|---|---|---|
| R1 | `claude -p --model <alias>` の alias | `fable` `opus` `sonnet` `haiku` の 4 つとも通り、`modelUsage` に claude-fable-5-1 / claude-opus-5-5 / claude-sonnet-5-5 / claude-haiku-5-5 |
| R2 | `--effort`・`--permission-mode`・`--setting-sources`・`--append-system-prompt-file` を `-p` と一緒に | 通る（引数エラーなし。`--help` の choices どおり） |
| R3 | **①答えが返るか**（K のホームで `--model fable --effort high --permission-mode auto --permission-prompts none --setting-sources user,project,local --append-system-prompt-file`。本文「今日の気分を一言で教えて」） | 返る（`is_error: false`・1 ターン・54 出力トークン） |
| R4 | **②入力トークンと秒数**（R3 と同じ） | 入力 **66,772 トークン**（`cache_creation_input_tokens`。`input_tokens` 2）。**送ってから JSON が出るまで 23.5 秒**（`duration_ms` 10.6 秒・`duration_api_ms` 5.8 秒＝残り約 13 秒は起動と K の SessionStart hooks・MCP の立ち上げ）。縮める前（`--safe-mode`・sonnet）は 787 トークン・3.4〜9.8 秒だった（`.steering/20261021-phase3-chat/evidence.md`） |
| R5 | 道具を使う送信（「作業フォルダのファイル名を 3 つ」。auto） | 2 ターンで答える（ls 相当を実行）。23.0 秒・入力 54,997 作成＋90,510 読み |
| R6 | **③承認のカード**: `--permission-prompts none` で `PermissionRequest` hook は走るか（使い捨てのプロジェクト設定に印を書く hook を置き、manual でファイルを書かせる） | **走る**。hook が `{}` を返すと `permission_denials` に Write が入り（拒否）、hook が `allow` を返すと **ファイルが作られた**＝hook の決定は `none` でも効く |
| R7 | K の常駐ペット（main の pet-hook）で同じ送信（manual・Write） | 13.1 秒で拒否（`permission_denials: [Write]`）。常駐ペットは hook の到着をログに書かないので、カードが出たかは R8 で見る |
| R8 | 自分の exe（この branch・`PET_TRACE=1`）を常駐させ、ペットのチャット欄から manual でファイルを書かせる | **カードが出た**（送信 9.7 秒後。下の「実機（③）」） |

秒数の内訳の見方: `claude -p` の起動は約 3 秒（evidence.md）。同じ環境では K の `~/.claude/settings.json` の SessionStart hooks（セッション一覧・リマインド等）と MCP（Notion・Slack・rea など）の立ち上げが加わり、API を呼ぶ前に **約 13 秒**かかる。モデルを軽くしても縮まらない（R4・R5 で sonnet/low でも 13〜16 秒）。

## 作り

| 層 | 変更 |
|---|---|
| `pet_core::chat` | `Model`（opus/fable/sonnet/haiku）・`Effort`（low/medium/high/xhigh/max）・`PermissionMode`（auto/manual/acceptEdits/plan/bypassPermissions）・`ChatOptions`（既定 Fable・High・Auto。`from_state` は無い・知らない値を既定に倒す）。`child_args(file, resume, options)` は `-p --output-format json --model <m> --effort <e> --permission-mode <p> --permission-prompts none --setting-sources user,project,local --append-system-prompt-file <話し方> [--resume id]`。`MODEL` 定数と `--safe-mode`・`--tools ""`・`--disallowedTools`・`--strict-mcp-config`・`disableAllHooks`・`--disable-slash-commands`・`--max-turns 1`・`--system-prompt-file` は外す。`work_dir(configured, home, fallback, is_dir)`: 設定のフォルダ → ホーム → データフォルダ |
| `pet-app::state` | `state.json` に `chat_model`・`chat_effort`・`chat_mode`・`chat_dir`（`chat_auth` と同じ置き方） |
| `pet-app::chat` | `ChatSetup`（接続・既定・作業フォルダ）で送信。チャット欄のメニューの値は `OVERRIDE`（静的）に持ち、`options()` で既定に重ねる。「新しいチャット」で消す。`SYSTEM_PROMPT` は「道具は Claude Code と同じように使える」に書き換え（置き換えでなく追加なので「道具が無い」と言わせない）。作業フォルダは起こす直前に `work_dir` で決める（試験ではホームを使わない） |
| `pet-app::pickdir`（新規） | `IFileOpenDialog`＋`FOS_PICKFOLDERS` のフォルダ選択（`spawn_blocking` で） |
| `pet-app::main` | コマンド `chat_defaults_set`・`chat_dir_set`・`chat_dir_pick`・`chat_options_set`・`chat_menu`。試験の手順に `opt:<model>,<effort>,<mode>` |
| `pet-app::ui` | チャット欄の島 48→**80px**（入力 48＋切替の行 32）。メニューが開いたら `set_chat_menu_height` で窓をその分（答えの欄より高い分）広げる。`chat_state` に `options` |
| `ui/options.js`（新規） | モデル・エフォート・モードの一覧と文言（設定とチャット欄で共有） |
| `ui/chat.html` | 下の行に左「自動」・右「Fable 5.1 高」。押すと上に伸びるメニュー（モード: 見出し＋名前＋説明＋番号＋チェック／モデル: 4 行＋区切り＋「エフォート 高 ›」→ 同じ島でエフォート 5 行に替わる）。数字キーで選べる・Esc と外側で閉じる |
| `ui/settings.html` | 「チャット」にモデル・エフォート・モードのポップアップと作業フォルダ（「選ぶ」「既定に戻す」） |

## 実機（③ 承認のカード）: 鳴る

R8（2026-10-28 00:38）。常駐ペット（main の exe）を止め、この branch の exe を本物のデータフォルダ（`PET_CLAUDE_DATA_DIR=%LOCALAPPDATA%\pet-claude`・`PET_CLAUDE_APP_PATH=<この exe>`＝K の hooks に登録されている main の pet-hook がこの exe を相手と認める）で `PET_TRACE=1` で起こし、試験の手順でチャット欄から sonnet・low・**manual** にして「pet-chat-test.txt を Write ツールで作って」を送った。`%LOCALAPPDATA%\pet-claude\logs\pet.log` の抜粋:

```
00:38:44.098 chat send (81 字, auth=subscription, model=sonnet, effort=low, mode=manual, resume=false)
00:38:50.857 wire event kind=prompt
00:38:53.080 wire event kind=tool_start
00:38:53.393 wire event kind=permission
00:38:53.393 permission requested id=e82d54c20c30b926 tool=Write
00:38:53.428 show -> sheet Waiting
00:38:53.814 bubble window created          ← 承認のカード
00:39:49.827 wire event kind=stop           ← hook の 60 秒待ちが切れ、none なので拒否
00:39:51.462 chat answer in 67.4s (95 字)
```

| 項目 | 値 |
|---|---|
| 送ってからカードが出るまで | **9.7 秒**（起動＋hooks＋MCP） |
| カードが出てから答えまで | 60 秒（pet-hook の timeout。誰も押さなかった）＋2 秒 |
| 答え | 95 字（作れなかった旨）。ファイルは作られていない（拒否） |
| 画面 | `%TEMP%\petchat-measure\live3-shot.png`（送信 42 秒後。ペットの上に「許可しますか？ Write …」のカード） |

結論: **同じチャットでもペットの承認のカードは鳴る**（R6 のとおり `--permission-prompts none` でも `PermissionRequest` hook は走り、K が「許可」を押せば通る）。`--permission-prompts host`＋`--permission-prompt-tool` への切替は要らない。manual の時、K が押さなければ 60 秒待って拒否になるので、ペットのチャットで道具を使わせるなら既定の auto（Claude が決める）が実用的。確認後は常駐ペットを Windows タスク `pet-claude` で main の exe に戻した（00:41:34 start）。

見つけた穴（別作業の提案・この作業では直さない）: 画面のとおり、Write のカードは「全文が見えないので、許可は Claude Code で」＋「Claude Code で」「拒否」で、**「許可」が無い**（要約が元の操作と同じでない道具は一押しで通さない規則。§5.2 の 5）。ペットのチャット（`-p`）には答える画面が無いので、manual で Write / Edit を頼むと拒否しかできない。提案: 要求の来た先がペット自身のチャット（`session_id` が今のチャットのもの）なら、K が頼んだ操作そのものなので「許可」を出し、「Claude Code で」を隠す。

## 送信ごとの待ちが 15 秒を超える → 子を常駐させる案（設計だけ。K の判断待ち）

R4 のとおり同じ環境では 1 送信 23 秒（うち約 13 秒が起動＋hooks＋MCP）。`claude -p --input-format stream-json --output-format stream-json` で子を 1 本起こしたままにすれば、2 通目からは起動と MCP の分が消え、API の時間（5〜12 秒）だけになる見込み。

- **起こす**: チャット欄を開いた時（`prewarm` と同じ所）に `claude -p --input-format stream-json --output-format stream-json --verbose --include-partial-messages`＋同じ引数で起こし、標準入力を開いたまま持つ。Job Object・`CREATE_NO_WINDOW`・環境の掃除は今と同じ
- **送る**: 標準入力に `{"type":"user","message":{"role":"user","content":"<本文>"}}` を 1 行。`--resume` は要らない（同じプロセスが会話を持つ）。モデル・エフォート・モードの切替は**この経路では起こし直し**（引数でしか渡せない。切り替えた送信だけ 23 秒）
- **受ける**: 標準出力の `{"type":"result"}` 行を今の `parse_output` に通す（`session_id` も同じ場所にある）。途中の `assistant` 行は捨てる（吹き出しは全文が決まってから）
- **止める**: 「新しいチャット」・時間切れ・終了で Job を閉じる。暇で 10 分たったら止める（AivisSpeech と同じ）。落ちていたら次の送信で起こし直す
- **費用**: 常駐中は K の MCP が全部起きたまま（Notion・Slack・rea のプロセス）。メモリは測っていない。起こしっぱなしの子にも hooks（SessionStart 1 回）は鳴る
- **判断を渡す 1 論点**: 「2 通目から約 10 秒短くなる代わりに、チャット欄を開くと K の MCP が全部起きたままになる」を受けるか。before＝毎回 23 秒・何も残らない／after＝初回 23 秒・2 通目から 5〜12 秒・MCP が常駐

## 非スコープ

- `--permission-prompts host`＋`--permission-prompt-tool`（SDK の承認の経路）への切替。R6 のとおり `none` でも `PermissionRequest` hook の決定は効くので、ペットの承認カードは今の hook の経路で成り立つ
- 子の常駐（上の案。K の判断の後）
- クラウド（`--environment`）
- 「他のモデル ›」（Claude アプリにはあるが、alias の 4 つ以外は出さない）
- K の `~/.claude/settings.json` の書き換え・main へのマージ

## 守ったこと

- 作業は worktree `pet-claude-feat-chat-same-env`（branch `feat/chat-same-env`）だけ。main の exe は上書きしていない
- `private.md`・鍵・認証情報は読んでいない・書いていない。実測の作業フォルダは K のホームと `%TEMP%\petchat-measure`（印を書く hook の設定はそこだけ）
