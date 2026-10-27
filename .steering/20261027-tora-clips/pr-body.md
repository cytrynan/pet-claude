## 何を
3 体目のペット「とらちゃん」（茶トラねこ・id `tora`・呼びかけ「とらちゃん」）を `assets/pets/tora/` に足す。シート（`build_pet`）＋ Comfy Cloud Wan 2.2 14B FLF2V の動画クリップ 13 本（ループ 4・つなぎ 7・単発 2）。

## なぜ
- K 指示（2026-10-27）: `茶トラ02_状況別イラスト_20261027.zip` から、スキル `make-pet-clips` の手順で作る。
- 2026-10-27 K 決定「次のペットから 960 生成＋ `fullCell: true`（等倍で持つ）」を初めて適用した 1 体。まめドラゴン・りんりんは 640・3/4 のまま触らない。
- `sprite.rs` のテストで同梱 `pet.json` が `fullCell: true` であることを、`clips.rs` のテストで 13 本のコマ数とファイル数の一致を固定する（手で数えない）。

## 検分（`.steering/20261027-tora-clips/tasklist.md` に表）
- 13 本すべて 始点差 ≤ 3.5・終点差 ≤ 6.5。手なし・顔と縞は全コマ保持（contact.png 目視）。
- 1 回目 4 本の後に K 判定「この出来で残りも同じ設定で進めてよい」。なでられは 3 候補から b を採用。
- 無料枠: 1 回目は前アカウントの最後の 1 回、2・3 回目は K が切り替えたアカウント（5/5 → 3/5）。
- 肌色 max の指標は茶トラの毛色が肌色に入るため使えない → SKILL.md に注記。

## 実機・ゲート
- `pet-switch.sh`（target-tora）: 3 項目 PASS（起動まめドラゴン → tora loaded 795 ms・clips 13 → state.json）。
- `cap_trial.ps1`（2880×1800・env 無し）: held_cell **384×416**（`fullCell: true` が効く）・専有 37.5MB（N1 50MB 以内）。
- `gate.sh`: GATE OK（2 commit とも gate-commit 経由）。新テスト: `tora_manifest_is_full_cell_with_clips_and_wake_word`、`bundled_pets_clips_json_match_frame_files` に tora、`align_poses.fits_in_canvas`。

## 2 commit 目（K の指摘「つままれてる時のやつ耳出てない？」）
- 姿勢画像のつままれるが上 62px 外に出ていた（吊り 560 のまま・茶トラは尻尾が垂れて縦に長い）→ `--floor dragged=612` で作り直し、つままれ中・置かれるの 2 本だけ再生成（1 回消費・残り 2/5）。
- 下げた分、尻尾の先が床の下（コマに 12px）に当たって自動の共通倍率が 0.40 まで縮んだので、`clips-src.json` に `scale: 0.60` を決め打ち（眠る・起きるは切れず、つままれ中の揺れで尻尾の先が最大 6px 出るだけ。倍率ごとのはみ出し表は tasklist）。
- 再発防止: `align_poses.py` に「絵がキャンバスの縁に触れていない」検査 `fits_in_canvas` を足して止まるようにした。

## 入れていないもの
zip・展開した静止画・姿勢画像・元の動画・workflow・証拠スクショ（`.git/info/exclude`）。

## マージ
K の判断で。ベースは `feat/rinrin-clips`（PR #58。#58 がマージされたら GitHub が `main` に付け替える）。

🤖 Generated with [Claude Code](https://claude.com/claude-code)
