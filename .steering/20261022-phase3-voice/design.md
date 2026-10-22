---
type: Steering
description: Phase 3 後半（読み上げ・音声入力）の設計。Q1〜Q3 は推奨案で書いた（K の答えで変える節は §6）
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-22T09:45:00+09:00 }
---

# 設計: 読み上げ（AivisSpeech）と音声入力（faster-whisper）

## 0. 手元の実測（2026-10-22・Ryzen 7 8845HS・GPU の CUDA なし）

| 項目 | 値 |
|---|---|
| AivisSpeech の起動 | `dev\voice\_engines\AivisSpeech\AivisSpeech-Engine\run.exe`、`/version` = 1.0.0、メモリ約 3.4GB（tasklist） |
| 天深シノ・ノーマル | style id `1063997408`（`/speakers` から名前で引く。先頭のスタイルに逃げない＝kuroko-voice の教訓） |
| 読み上げの速さ | 4 字「できたよ」0.36 秒、48 字の答え 2.2 秒（audio_query＋synthesis） |
| 文字起こし（6 秒の発話・CPU int8） | small: 読み込み 1.7 秒＋認識 1.6 秒。large-v3: 7.5 秒＋7.4 秒。どちらも「まめちゃん」を「もめちゃん」と聞いた |
| モデル | `Systran/faster-whisper-small` は HF のキャッシュに完全な形である（medium は model.bin が欠けている） |
| マイク | マイク配列（Senary Audio）1 つ |

## 1. 純粋な部分（`pet-core::voice`・テスト先行）

| 関数・型 | 役目 |
|---|---|
| `speech_for_answer(answer) -> Option<String>` | チャットの答えから話す 1 行を作る。`chat::bubble_text` の結果をそのまま使い、吹き出しに出せない時（`BUBBLE_HIDDEN`）は「答えはチャット欄に出したよ」。末尾の「…」は落とす |
| `SpeechQueue` | 話す順の決まり（V3）。`push(Utterance, now)`・`next(now)`・`finished()`。固定文言は話し中なら捨てる、同じ文言は 15 秒以内に繰り返さない。答えは 1 つだけ待たせる（新しい答えが来たら古い方を捨てる） |
| `Endpointer` | 録音の区切り（V5）。16kHz・16bit の 30ms ごとの音量（RMS）を受け、最初の 300ms で静かさを測り、その 3 倍（下限あり）を超えたら話している。話し始めて 300ms 以上の後に 1000ms 黙れば `Done`、5 秒話さなければ `NoSpeech`、15 秒で `MaxLength` |
| `transcript_text(raw) -> Option<String>` | 文字起こしの結果を送れる文にする（V7）。前後の空白を落とし、空・幻聴の決まり文句（「ご視聴ありがとうございました」等の一覧）だけの文は `None`。長さは `chat::input_text` と同じ規則 |
| `wav_bytes(samples, rate)` | 16bit モノラルの WAV を組み立てる（文字起こしに渡すファイル） |
| `parse_wav(bytes) -> Option<WavInfo>` | AivisSpeech が返した音の頭を確かめる（RIFF・WAVE・PCM・長さが合う）。壊れていたら鳴らさない（V2） |

## 2. 読み上げ（`pet-app::tts`）

- 専用のスレッド 1 本と通り道（channel）。呼び出し側（吹き出しを出す所）は文を投げるだけで待たない
- HTTP は標準ライブラリの `TcpStream` で `127.0.0.1:10101` に直接（新しい crate を足さない）。つなぐ・読む時間の上限 2 秒（`/version`）・30 秒（`/synthesis`）
- スタイル id は最初に `/speakers` で「天深シノ」「ノーマル」を名前で引いて覚える。見つからなければ話さず、ログに 1 行
- 鳴らすのは `PlaySoundW(SND_MEMORY | SND_SYNC | SND_NODEFAULT)` を同じスレッドで。止めるのは別スレッドから `PlaySoundW(NULL)`（「声で知らせる」をオフ・終了）
- **エンジンの起動（Q2 の推奨）**: 話す時に `/version` が返らなければ `run.exe` を `CREATE_NO_WINDOW` で起こし、Job Object（閉じると止まる）に入れる。起きるまで最大 90 秒待ち、その間に来た固定文言は捨てる（答えは待たせる）。ペットが起こした時だけ、最後に話してから 10 分で止める。既に動いていたエンジンには触らない。exe の場所は既定の `%USERPROFILE%\dev\voice\_engines\AivisSpeech\AivisSpeech-Engine\run.exe`、無ければ起こさない
- 話す場所: `ui::show_chat_answer`・`show_chat_failure`（失敗は「うまくいかなかったよ」）・`pet_window::on_wire_event` の固定文言・`ui::show_permission`
- ログ: 話した字数・合成の秒数・エンジンを起こした/止めた。文そのものは書かない

## 3. 音声入力（`pet-app::voice`）

- バーの右のボタン（`ui_action("voice")`）で始める。もう一度押すと止める（送る）。録音中はボタンが赤く脈打ち、ペットは「呼びかけ」のポーズ（承認待ちと同じ見た目）
- 録音は `waveIn*`（winmm）で 16kHz・モノラル・16bit を直接受ける（Windows が変換する。新しい crate を足さない）。30ms のバッファを回し、`Endpointer` に渡す
- 止まったら `%LOCALAPPDATA%\pet-claude\voice\rec-<世代>.wav` に書き、録音の前に起こしておいた文字起こしの子にパスを渡す（2026-10-22 に変更。下の「話し出すまで」）。**終わったら成否に関係なく消す**（V9）。起動時に残っていれば消す
- 文字起こしの子: `%LOCALAPPDATA%\pet-claude\stt\venv\Scripts\python.exe -I <repo>\scripts\stt\transcribe.py`（録音を始める前に起こしてモデルを読ませ、WAV のパスは録り終えてから標準入力の 1 行目で渡す。取りやめならパスを渡さずに閉じる）。環境は `HF_HUB_OFFLINE=1`（実行中にダウンロードしない）。中身は faster-whisper small・`device="cpu"`・`compute_type="int8"`・`language="ja"`・`beam_size=1`・`vad_filter=False`（区切りはこちらで済んでいる）・`condition_on_previous_text=False`。標準出力に `{"text": ..., "secs": ...}` を 1 行
- 子の起こし方はチャットと同じ（止めて起こして Job に入れる・出力の上限・90 秒で時間切れ（録音とモデルの読み込みを含む）・取り消し・世代番号）。`chat.rs` の `run_child`・`JobGuard`・`new_kill_on_close_job`・`read_capped` を `child.rs` に移して両方から使う（振る舞いは変えない。チャットの故障注入 18 項目で確かめる）
- 結果を `transcript_text` に通し、`Some` なら **チャット欄に K の発言として出してから** `chat::submit` に渡す（Q3 の推奨）。`None` なら吹き出しに「聞き取れなかったよ」
- 失敗の吹き出し: マイクが無い「マイクが見つからないよ」、拒否「マイクを使えないよ（Windows の設定）」、道具が無い「音声入力の準備が要るよ（設定を見てね）」、時間切れ「文字起こしが終わらなかったよ」
- 同時に 1 本だけ。チャットの返答待ちの間は始めない（「まだ考え中だよ」）

## 4. 準備のスクリプト（Q1 の推奨）

- `scripts/setup-stt.ps1`: `py -3.10 -m venv %LOCALAPPDATA%\pet-claude\stt\venv` → `pip install faster-whisper==1.2.1`（いま手元で動いている版）→ small が HF のキャッシュに無ければ 1 回だけ取りに行く → 短い無音で 1 回走らせて確かめる
- K の操作は「設定画面の案内どおりに 1 行を PowerShell で貼る」だけにする（くろこが代わりに走らせてもよい）

## 5. 設定画面

- 「声」の囲みに 2 行の状態を足す: 「読み上げ: 使えます／エンジンは話す時に起こします／エンジンが見つかりません」「音声入力: 使えます／準備が要ります（`scripts\setup-stt.ps1`）／マイクが見つかりません」
- 既存の「呼びかけで音声入力を始める」の行と、ペットの囲み（ウェイクワードの表示）は触らない（別スレッドの持ち分）。注記「声の機能は Phase 3 以降で使えます」を「呼びかけは Phase 4 で使えます」に直す
- `state.json` は増やさない（`tts` は既にある）

## 6. K の答えで変わる所

| 問い | 推奨と違う答えなら |
|---|---|
| Q1 | B（whisper.cpp）: §3 の子を `whisper-rs` の呼び出しに置き換え、§4 を「cmake・LLVM の導入とモデルのダウンロード」にする |
| Q2 | B（動いている時だけ）: §2 の起動・停止を消し、設定の状態は「エンジンが動いていません」 |
| Q3 | B（直してから送る）: §3 の最後を「チャット欄を開いて入力に入れる」に変える |

## 7. 検証

- 単体: `crates/pet-core/tests/voice.rs`（テスト作成係と実装係を分ける）
- 故障注入: `scripts/verify/voice-fault.py`（偽のエンジン＝遅い・壊れた WAV・止まっている、偽の文字起こし＝時間切れ・壊れた出力・幻聴、録音は `PET_TEST_VOICE_WAV` で決まった WAV を流し込む。試験用の差し替えはデバッグビルドで `PET_CLAUDE_DATA_DIR` を指定した時だけ効く＝チャットと同じ）
- 実機: K の声で S11（「まめちゃん、明日の天気どうかな」→ 答えが吹き出しと声で返る）、「声で知らせる」のオフで止まる
