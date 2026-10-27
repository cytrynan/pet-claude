---
type: Steering
generated: { by: kuroko/claude-fable-5-1, at: 2026-10-28T01:30:00+09:00 }
---

# 内蔵の仮キャラ（placeholder）を外す（T1）

## brief
- K 指示（2026-10-28）「仮キャラの削除を行って」。ペットが 4 体（まめドラゴン・りんりん・とらちゃん・しばちゃん）そろい、動作確認用の仮キャラが設定画面の選択肢に出る意味が無くなった。
- 仮キャラは exe に埋め込んだ最後の手段（読めるペットが無い時の起動用）だった。外した後は、1 つも読めなければ**理由をダイアログに出して起動しない**（ログだけだと「起動しない」にしか見えない）。試しの置き場所（`PET_CLAUDE_DATA_DIR`）ではダイアログを出さない（故障注入が止まる）。
- 変更: `pets.rs`（`BUILTIN_*`・`include_bytes!` を外す。`PetEntry.dir: PathBuf`。`discover_in`・`startup_order`・`load`・テスト 4 本）／`main.rs`（`show_startup_error`）／`state.rs`・`ui/settings.html`（コメント）／`gen_assets`（アイコンだけ）／`assets/pets/placeholder/` 削除／`pet-switch.sh`（既定の切替先 `rinrin`）／`render-ui.js`（場面 `settings-no-wake-word`）／docs（architecture §4.4・README・log）。
- 対象外: `pet_core::placeholder` モジュール（アイコンの絵に使う）。

## タスク
- [x] 仮キャラをコードと資産から外し、起動できない時のダイアログを足す (#109)
- [x] ゲート GATE OK（pet-app のテスト 38 本・pet-core 109 本ほか）。故障注入: `--pet Z:\nope` で起こすと「ペットを読めないので起動できません。pet.json: 指定されたパスが見つかりません。(os error 3)」のダイアログが出てペットは出ない（`startup-error.png`・ログ「スプライトを読めない」）。設定画面の一覧の実機スクショは未（下の止まった点）
- [x] docs（architecture §4.4・README・log.md）→ gate-commit → PR（マージは K）

## 止まった点
- 試しの置き場所（`PET_CLAUDE_DATA_DIR`）の mutex を別セッション（chat-menu-blink）の試しインスタンスが持っていて、設定画面の一覧の実機確認（仮キャラが無い）はこの作業では撮れなかった。一覧は `pets::discover` の結果そのもので、`discover_in` のテストが仮キャラ無しの並びを確かめている。PR のマージ後に常駐ペットで目視する
- ダイアログの故障注入は常駐ペット（main の mutex）を止めて行った。OK を `winctl click` で押しても閉じなかった（別セッションのチャット欄が最前面で重なっていた疑い）ので PID で止め、常駐はタスクで起こし直した
