---
type: Steering
description: Phase 4 の設計。自動起動は schtasks＋XML、ウェイクワードは openWakeWord の特徴抽出＋小さな頭を AivisSpeech の合成と K の録音で学習
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-23T02:45:00+09:00 }
---

# 設計: Phase 4

## 1. 自動起動

- 中身は `pet_core::autostart`（純粋。テスト先行 12 本）: `task_xml`（XML を組む）・`registered_exe`（読み戻す）・`view`（いまの exe と比べる）・`decode_output`（schtasks の出力の文字コード）
- `crates/pet-app/src/autostart.rs`: `schtasks /Create /TN pet-claude /XML <file> /F`・`/Delete`・`/Query /XML` を窓なしで起こす。XML は宣言どおり UTF-16LE＋BOM で書いてすぐ消す
- 状態は `state.json` に持たない。タスク スケジューラが実物で、設定画面は開くたびに読む（K が手でタスクを消しても画面がずれない）
- 既定から変える所: `ExecutionTimeLimit PT0S`（既定 72 時間で常駐が止まる）・`DisallowStartIfOnBatteries false`・`StopIfGoingOnBatteries false`・`Priority 4`（既定 7＝通常未満）・`MultipleInstancesPolicy IgnoreNew`・ログオンから 10 秒待つ
- ユーザーは `USERDOMAIN\USERNAME`（全ユーザーのログオンにすると登録に管理者が要る）。`run-hidden.vbs` は K の規範の置き場所 `%USERPROFILE%\.claude\scripts\run-hidden.vbs` を使い、無ければ登録しない（理由を画面に出す）
- CLI: `--install-autostart [--task-name <名前>] [--disabled]`・`--uninstall-autostart`・`--autostart-status`。試験は無効のタスクを別名で作ってすぐ消す（ログオンで発火させない）

## 2. ウェイクワード

### 2.1 方式（Rust の `ort` ではなく Python の子）

openWakeWord と同じ形: 共通の特徴抽出（`melspectrogram.onnx` → `embedding_model.onnx`。80ms ごとに 96 次元）＋ウェイクワードごとの小さな頭（直近 16 個＝1.28 秒を見て 0〜1）。

- 子は声のスレッドで入れた環境（`%LOCALAPPDATA%\pet-claude\stt\venv`。numpy・onnxruntime がある）で動かす。pip で足す物はない。openwakeword のパッケージも入れず、前処理（`x/10+2`・76 フレームの窓・8 フレームずつ）を `scripts/wake/oww.py` に書く
- Rust の `ort` を選ばない理由: exe に onnxruntime の DLL（約 10MB）とビルド時のダウンロードが増える。文字起こしが既に Python の子なので、同じ作法（`-I`・Job Object・窓なし）に揃える
- マイクは pet-app が録って PCM（16kHz・モノラル・16bit）を子の標準入力に流す。子は検出したら 1 行の JSON を書く。マイクを開くコードは声のブランチの `voice.rs` にあるので、つなぐのは PR #25 のマージ後

### 2.2 学習データ

| 種類 | 作り方 | 置き場所 |
|---|---|---|
| 正（合成） | AivisSpeech で「まめちゃん」を、入っている話者×スタイル×速さ・高さ・抑揚を振って合成 | `%LOCALAPPDATA%\pet-claude\wake\data\<pet>\synth-pos\` |
| 正（K） | `scripts/wake/record-wake.ps1` で K が 20 回言う（2 秒ずつ）。14 回を学習、6 回を判定に回す | `…\wake\data\<pet>\k\` |
| 負（近い言葉） | 「まめ」「あめちゃん」「ままちゃん」「めちゃくちゃ」「まめまき」など、言い間違えやすい言葉を同じく合成 | `…\synth-neg\` |
| 負（会話） | 日本語の日常の文を合成し、つないで会話の流れにする | `…\synth-neg\` |
| 負（公開） | openWakeWord の `validation_set_features.npy`（特徴量で配布。約 11 時間）。前半を学習、後半を誤反応率の判定に使う | `…\wake\` |

正の音は 2 秒の窓の終わり近くに置き、雑音（白・ピンク・合成した会話）を混ぜて増やす（openWakeWord の学習と同じ考え方）。

### 2.3 頭と判定

- 頭は numpy の 2 層（1536→32→1）。重みは `…\wake\<pet>.npz`（K の声から作るのでリポジトリに入れない）
- 検出: 点数がしきい値を超えたフレームが 2 回続いたら反応し、2 秒は次を出さない
- しきい値は判定用のデータで「誤反応 ≤ 1 回/10 分」を満たす中で、K の判定用の録音の反応が最も多い値を選ぶ

## 3. 本番素材

K の Q2 次第。「はい」なら PRD U3 を決定にし、roadmap の Phase 4 から外す。
