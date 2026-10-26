## 1. 結論: 受入可

対象は HEAD `ed0f88d`＋未コミットの2ファイルの差分。**F1 の筋書きは修正後には成立しません。** 今回の差分に高・中の指摘はありません。実機未測など、1巡目の留保は継続します。

## 2. 指摘

指摘なし。取り出し・取消の順序、ロックの寿命、世代の受け渡し、提供ログを静的に照合した結果です。独自の実機再現は行っていません。

## 3. 「見てほしいこと」への答え

1. **F1 は回収済み。** 取り出しが先なら旧世代を保持して再生時に拒否され、取消が先なら待機文が消えます。`was/on` は true/false・false/false とも取消、true/true・false/true は世代を変えません。`prewarm` は合成のみ、`shutdown` は無効化して終了する経路で、今回の世代取得を迂回して復活させません。参照：[tts.rs](/C:/Users/meer0/dev/pet-claude/pet-claude-feat-phase3-voice/crates/pet-app/src/tts.rs:102) 102–155、183–236、281、312–347行。

2. **新たなデッドロックは見当たりません。** 105行の一時ガードは文末で解放され、109行の `PLAY` と同時保持しません。worker の条件変数待機は `QUEUE` を解放します。毒されたロックでも取消側は回収して世代更新・クリアを行いますが、worker が終了する既存挙動まで復旧する修正ではありません。参照：同ファイル102–114、180–214、312–347行。

3. **F1 の状態遷移は表しています。** 取り出し→取消で旧世代が無効、取消→取り出しで文なし、キュー未生成でも世代更新、を確認しています。ただし実際のロック競合・オフ→オン・再生を実行するテストではなく、呼出側との接続はコード照合で補っています。参照：同ファイル716–739行。

4. **S1 で合否条件は甘くなっていません。** 変更は終了を開始する待機条件だけで、生存・合成ログ・完了ログ・再生失敗なし・HTTP到達の条件は維持されています。完了しない入力は既存の上限30秒で待機を終え、完了ログがなければFAILです。参照：[voice-fault.py](/C:/Users/meer0/dev/pet-claude/pet-claude-feat-phase3-voice/scripts/verify/voice-fault.py:151) 151–198、223–228行。

## 4. 近道検査

今回変わった点だけを記します。

- **① テスト追加と試験修正あり。** 製品の受入基準変更は今回の差分にありません。「関数未定義でコンパイル失敗」という red は、旧実装でF1を再現した証拠とは区別します。
- **③ 提供ログが更新。** [gate2.log](/C:/Users/meer0/AppData/Local/Temp/claude/C--Users-meer0-dev-pet-claude-pet-claude-feat-phase3-voice/5c28dcda-20bf-4baf-b43c-0c3cc876ded2/scratchpad/codex-work/gate2.log:7) の31件成功・71–72行の `GATE OK`／exit 0、[voice-fault3.log](/C:/Users/meer0/AppData/Local/Temp/claude/C--Users-meer0-dev-pet-claude-pet-claude-feat-phase3-voice/5c28dcda-20bf-4baf-b43c-0c3cc876ded2/scratchpad/codex-work/voice-fault3.log:474) のtts-ok、V4の3項目、536–538行の30/30 PASS／exit 0を確認しました。
- **⑤ F1 の根拠が解消。** 世代をロック外で取得していた箇所がなくなり、取り出した世代が再生まで渡されています。

## 5. 任意

将来の回帰防止には、worker の取り出し直後で停止し、実際の `set_enabled(false)`→`set_enabled(true)` を挟む同期テストが有用です。今回の単体テストだけでは、呼出側が再びロック外で世代を取得する変更を検出できません。現修正の差し戻し理由にはしません。

## 6. 動かなかったコマンドと未確認の事項

- 初回の読み取りでプロファイル由来の `DotSourceNotSupported`、fnm のアクセス拒否、`Invoke-Expression` エラーが出ました。コマンド自体は終了コード0。以後は `login:false`・UTF-8指定で必要箇所を再読しました。
- `git --no-optional-locks diff --no-ext-diff`、ソース・ログの読み取りは終了コード0。差分は依頼どおり2ファイルでした。
- cargo・故障注入は指示どおり未実行。テスト結果は提供ログの照合です。実機の音・設定操作・毒されたロックの実行試験・現行版の性能値は未確認です。
- `shutdown` は既存どおり `PLAY` を取らず停止します。終了時の厳密な競合安全性まで、今回のF1修正で保証されたとは判断していません。

**ファイル変更なし。commit／push ともに未実施。**