# docs 更新ログ

> ドキュメント記録規約（OKF準拠。正本: `development-guidelines` スキルの `guides/recording.md`）。
> `docs/` 配下のドキュメントを作成・更新・廃止したら、同じ作業内でここに1行追記する。新しい順。
> 書式: `* **Creation|Update|Deprecation**: [タイトル](ファイル名.md) — 何をどう変えたか (アクター)`

<!-- /setup-project でドキュメントを作成したら、最初のエントリを**このコメントの下（閉じ記号の後）**に追記してください。
コメントの中に書くと Markdown の描画に出ません（実例: 「ここに追記」と読んでコメント内へ書き続け、ログ全体が見えないまま運用された）。例:
## 2026-10-28
* **Update**: [README.md](manual/README.md)・[pet-claude-manual.docx](manual/pet-claude-manual.docx) — マニュアルを apple-design（HIG）の値で組み直し。文章を半分に減らし、流れ・状態・時間軸の図 8 枚（diagrams.html → render-diagrams.js → images/diagrams/）と色つきチップで見せる。10 ページ・各章 1 ページ (kuroko/claude-fable-5-1 ← human:K)
* **Update**: [README.md](manual/README.md)・[pet-claude-manual.docx](manual/pet-claude-manual.docx) — マニュアルの正本を Word（pet-claude-manual.docx）に変更。PDF は export-pdf.ps1（Word の COM）で書き出す。manual.html・build-pdf.js は削除、初版を組む build-docx.js を追加 (kuroko/claude-fable-5-1 ← human:K)

## 2026-08-01
* **Creation**: [プロダクト要求定義書](product-requirements.md) — 初版作成・ユーザー承認済み (claude/claude-sonnet-5)
-->
## 2026-10-28
* **Update**: [技術仕様書](architecture.md) — §4.4「入っているペット」に 6 体目のごまちゃん（`goma`・ハムスター灰・960 生成＋`fullCell: true`。スキル `make-pet-clips`。寝姿は 0.9 倍の静止画から） (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [技術仕様書](architecture.md) — §4.4「入っているペット」に 5 体目のこむぎちゃん（`komugi`・ハムスター茶・960 生成＋`fullCell: true`。スキル `make-pet-clips`。寝姿は色と大きさを直した静止画から） (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [技術仕様書](architecture.md) — §4.4 内蔵の仮キャラ（`placeholder`）を外す。起動時に試す順から仮キャラを除き、1 つも読めなければダイアログを出して起動しない (kuroko/claude-fable-5-1 ← human:K)
* **Update**: [技術仕様書](architecture.md) — §4.4 起動時に試す順に「既定のまめドラゴン（`DEFAULT_PET_ID`）」を足す。`kuroshiba` がフォルダ順で先頭になり既定が変わったため (kuroko/claude-fable-5-1 ← human:K)

## 2026-10-27
* **Update**: [技術仕様書](architecture.md) — §4.4「入っているペット」に 4 体目のしばちゃん（`kuroshiba`・黒柴・960 生成＋`fullCell: true`。スキル `make-pet-clips`） (kuroko/claude-fable-5-1 ← human:K)
* **Update**: [技術仕様書](architecture.md) — §4.4「入っているペット」に 3 体目のとらちゃん（`tora`・茶トラねこ・960 生成＋`fullCell: true` の最初の 1 体。スキル `make-pet-clips` で作成） (kuroko/claude-fable-5-1 ← human:K)
* **Update**: [技術仕様書](architecture.md) — §4.5 の「次のペットから」に、手順と道具の正本としてプロジェクトスキル `make-pet-clips` を指した（2026-10-27 K 指示でスキル化） (kuroko/claude-fable-5-1 ← human:K)
* **Update**: [技術仕様書](architecture.md) — §4.4 に `pet.json` の `fullCell`、§4.5 に「次のペットから 960 生成＋等倍」（2026-10-27 K 決定）の作り方を追記 (kuroko/claude-fable-5-1 ← human:K)
* **Update**: [技術仕様書](architecture.md) — §4.5 に画質の試し（`PET_TEST_FULL_CELL`・3/4 と等倍のメモリ実測・960 生成の効き）を追記 (kuroko/claude-fable-5-1 ← human:K)
* **Update**: [技術仕様書](architecture.md) — §4.4 に 2 体目のペット「りんりん」（パンダ・`assets/pets/rinrin/`・動画クリップ 13 本）を追記。§4.5 の見出しにりんりんを足した (kuroko/claude-fable-5-1 ← human:K)
* **Update**: [技術仕様書](architecture.md)・[プロダクト要求定義書](product-requirements.md)・[DESIGN.md](../DESIGN.md) — 質問（AskUserQuestion）にペットの吹き出しで答える（§5.1 の表と §5.4・F21・質問のカードと設定の「Claude Code」） (kuroko/claude-fable-5-1 ← human:K)
* **Update**: [技術仕様書](architecture.md)・[DESIGN.md](../DESIGN.md) — §3-b に吹き出しの固定文言の大きさの決め方（字幅の見積もり・幅 180〜320・折り返して高さで取る・描画後の測定で広げる保険）、DESIGN.md の固定文言を文字に合わせて広げる形に (kuroko/claude-fable-5-1 ← human:K)
## 2026-10-26
* **Update**: [技術仕様書](architecture.md) — §7 に「端末の窓を出さない」（デバッグでも GUI サブシステム・CLI は AttachConsole・リダイレクト時はつながない） (kuroko/claude-fable-5-1 ← human:K)
* **Update**: [プロダクト要求定義書](product-requirements.md)・[ロードマップ](roadmap.md)・[技術仕様書](architecture.md) — ウェイクワードの道具（`scripts/wake/`）を git の外に（`.gitignore`）。履歴は 25f7b23・f2bac67 (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [プロダクト要求定義書](product-requirements.md)・[ロードマップ](roadmap.md)・[技術仕様書](architecture.md) — ウェイクワード（F20）を保留に。道具までで止め、ダウンロード・K の録音・マイクへの接続はしない (kuroko/claude-opus-5-5 ← human:K)

## 2026-10-24
* **Update**: [技術仕様書](architecture.md)・[DESIGN.md](../DESIGN.md) — 承認のカードの「ターミナルで」を要求の来た先で変える（アプリは「Claude アプリで」・分からない時は「Claude Code で」）。§5.2 に判定の規則（CLAUDE_CODE_ENTRYPOINT と、EXECPATH と CLAUDE_PID の実行ファイルの照合）と根拠、DESIGN.md にボタン行の余白と折り返さない規則 (kuroko/claude-opus-5-5)
* **Update**: [プロダクト要求定義書](product-requirements.md)・[ロードマップ](roadmap.md) — U3 を決定に（まめドラゴンを本番素材にする）。roadmap の Phase 4 の本番素材を決定済みに (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [技術仕様書](architecture.md) — §6 のウェイクワードを実装どおりに（Python の子・openWakeWord の特徴抽出＋ペットごとの頭・学習と判定の道具・置き場所・ライセンス CC BY-NC-SA 4.0） (kuroko/claude-opus-5-5)
* **Update**: [ロードマップ](roadmap.md) — Phase 4 のウェイクワードを「学習と判定の道具まで。数字はダウンロードと K の録音待ち」に (kuroko/claude-opus-5-5)

## 2026-10-23
* **Update**: [ロードマップ](roadmap.md) — Phase 4 の自動起動（実装済み）とウェイクワード（道具まで）の進み具合 (kuroko/claude-opus-5-5)
* **Update**: [技術仕様書](architecture.md) — §7 に自動起動（Windows タスク・run-hidden.vbs・schtasks の XML・CLI）を実装どおりに (kuroko/claude-opus-5-5)

## 2026-10-22
* **Update**: [技術仕様書](architecture.md) — §6.3 文字起こしのスクリプトは exe に埋め込んでデータフォルダへ書き出す（セキュリティレビューの指摘） (kuroko/claude-opus-5-5)
* **Update**: [プロダクト要求定義書](product-requirements.md) — F16・F17 を K の決定（Q1〜Q3 すべて A）に (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [技術仕様書](architecture.md) — §6.2・§6.3 に話し出すまでを縮める工夫（文ごとの合成・エンジンとモデルを先に起こす・文字起こしの子を録音の前に起こす）と前後の実測、ミュートの見分け方 (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [DESIGN.md](../DESIGN.md) — 色付けの例外に「録音中の音声ボタンは記号だけ赤で脈打つ」を足した (kuroko/claude-opus-5-5)
* **Update**: [技術仕様書](architecture.md)・[プロダクト要求定義書](product-requirements.md)・[ロードマップ](roadmap.md) — §6.2 読み上げ（SpeechQueue・手書きの HTTP・PlaySoundW・エンジンを起こして暇なら止める）と §6.3 音声入力（waveIn・Endpointer・faster-whisper small の子プロセス・WAV を残さない）、F16・F17 を実装どおりに (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [技術仕様書](architecture.md)・[プロダクト要求定義書](product-requirements.md)・[DESIGN.md](../DESIGN.md) — ウェイクワードをペットごとに持つ（pet.json の wakeWord・まめドラゴンは「まめちゃん」・設定画面のペットの下に出す・build_pet は手で足した項目を残す）、U2 を決定に (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [技術仕様書](architecture.md) — §3-b チャット欄の閉じ方を「ペットの左クリックだけ」に（フォーカスの喪失・外のクリック・Esc・ドラッグでは閉じず、ペットについて行く）。マウスフックはバーかメニューが出ている間だけ (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [技術仕様書](architecture.md)・[DESIGN.md](../DESIGN.md) — チャット欄をバーと同じ規則で置く（ペットの真下、収まらなければ真上。答えの欄で高くなっても置き直す） (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [技術仕様書](architecture.md) — §6.1 にチャット欄とペットの重なり（チャット欄が出ている間はペットが最前面を取り直さない・開いた直後の前面の喪失を 1 回取り直す）。実機確認で見つかった 2 件の修正に合わせた (kuroko/claude-opus-5-5 ← human:K)

## 2026-10-21
* **Update**: [技術仕様書](architecture.md)・[DESIGN.md](../DESIGN.md) — §6.1 を検証係の指摘の回収に合わせた（止めた状態で起こして Job に入れる・標準入力は別スレッド・本文 2000 字・予約は答えを受けてから外す・チャット欄でも秘密を伏せる・失敗の理由・確かめるの時間切れ）、最初の返事を待つ間の答えの島 (kuroko/claude-opus-5-5)
* **Update**: [技術仕様書](architecture.md)・[DESIGN.md](../DESIGN.md) — §6.1 チャット（claude -p の子プロセス・--safe-mode・--model sonnet・認証の切り替えと子の環境・Job Object・1 本だけ・表示の検査）、§7 に chat_auth と資格情報マネージャー、答えの吹き出し・チャット欄の答え・設定のチャットの欄 (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [プロダクト要求定義書](product-requirements.md) — U1 を決定に（新しいセッション・`claude -p` の子プロセス・ツールなし・認証はサブスク／API キー切替で既定サブスク）、F18・F19 に反映 (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [技術仕様書](architecture.md) — §4.4 pet.json のパスはフォルダの中の相対パスだけ・クリップ名の文字の制限（セキュリティレビューの指摘） (kuroko/claude-opus-5-5)
* **Update**: [技術仕様書](architecture.md) — §3 窓に描く DIB を使い回す（R5 の待機の CPU 2.81% → 1.56〜1.88%）、§4.5 の実測表を更新 (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [技術仕様書](architecture.md) — §4.5 動画クリップ（clips.json・build_clips.py・Player の計画・持ち方の上限と固定・実測）、§4.2 クリックは Petted（なでられ） (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [技術仕様書](architecture.md) — §3-b バー: 作業領域の下端でペットの下に収まらない時はペットの上に出す（実機確認でバーがペットの裏に隠れていた） (kuroko/claude-opus-5-5)
* **Update**: [技術仕様書](architecture.md)・[プロダクト要求定義書](product-requirements.md)・[DESIGN.md](../DESIGN.md) — カーソルが乗ったら出すのは右クリックのメニューでなくバー（K の訂正）。乗って出たバーはクリックで固定 (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [技術仕様書](architecture.md) — §4.4 メモリ: 中身のあるコマだけを切り抜いて持つ（K の大きさ 105% で 58.6MB → 31.6MB） (kuroko/claude-opus-5-5)
* **Update**: [技術仕様書](architecture.md) — §4.4 build_pet の `--frames`（描いたアニメーションのコマを行に使う・合わせた枠で切り出す・静止画の大きさに合わせる）、まめドラゴンの歩きを 8 コマに (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [技術仕様書](architecture.md) — §4.4 呼吸をアプリが描くたびに計算する（pet.json の breathe・16 段階・待機 3.6 秒/睡眠 5.2 秒）、動かない行は 1 コマ (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [技術仕様書](architecture.md) — §4.4 に読み込みの 1 本化（kick_load）、行の有無とコマ数をシートから測る（embed-pet の取り込み）、メモリが N1 を超えうる条件 (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [技術仕様書](architecture.md)・[プロダクト要求定義書](product-requirements.md)・[DESIGN.md](../DESIGN.md) — ペットの素材と切り替え（pet.json の cellScale・探す場所・起動時の選び方・設定画面のポップアップ・メモリのための縮小・静止画からシートを作る build_pet） (kuroko/claude-opus-5-5 ← human:K)
* **Creation**: [embed-pet 解析](embed-pet-analysis.md) — JinTanba/embed-pet（Apache-2.0）と比較。推す取り込み 1 つ（行の有無とコマ数をシートのアルファから決め、空の行は代替へ）と見送り 8 件、まめドラゴンの行の当てはめ、切り替え時の測り直し (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [技術仕様書](architecture.md)・[DESIGN.md](../DESIGN.md) — 発見ファイルの確かめ方（実行ファイルのフルパスと生成時刻）、承認のカードの折り返しと 600ms の押下防止 (kuroko/claude-opus-5-5)
* **Update**: [技術仕様書](architecture.md) — §5 を全範囲の検証の回収に合わせた（許可を出すツールと項目の限定・文字の許可リスト・出たばかりのカードへの押下を捨てる・下げたカードに ask・発見ファイルの pid の確認・PET_AUTO_DECIDE の条件） (kuroko/claude-opus-5-5)
* **Update**: [技術仕様書](architecture.md)・[プロダクト要求定義書](product-requirements.md)・[DESIGN.md](../DESIGN.md) — カーソルがペットに乗ったらメニューを出す（200ms 待つ・離れて 400ms で閉じる） (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [技術仕様書](architecture.md) — §5 を検証係の指摘の回収に合わせた（許可を出す 3 条件・放置した承認の失効と見回り・接続ごとのスレッドと要求全体の期限・乱数が取れなければ受け口を開かない） (kuroko/claude-opus-5-5)
* **Update**: [技術仕様書](architecture.md)・[ロードマップ](roadmap.md)・[DESIGN.md](../DESIGN.md) — Phase 2 を実装どおりに（hook の登録内容・承認の往復・許可を出さない条件・IPC・間引きを置かない理由）、吹き出しの部品と「半透明の面は値より薄く写る」 (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [プロダクト要求定義書](product-requirements.md)・[技術仕様書](architecture.md)・[DESIGN.md](../DESIGN.md) — 放置したときの散歩を設定でオン・オフ（既定オフ） (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [プロダクト要求定義書](product-requirements.md) — N1（メモリは子孫プロセス込みの専有で判定。普段 50MB・UI 表示中 150MB）を K 承認済みに (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [技術仕様書](architecture.md)・[ロードマップ](roadmap.md)・[プロダクト要求定義書](product-requirements.md) — メモリの数値を実測の幅（専有とワーキングセット）に。N1 の改訂は K 未承認と明記（評価係の指摘） (kuroko/claude-opus-5-5)
* **Update**: [技術仕様書](architecture.md) — §3-b の「後回し」の説明を訂正（run_on_main_thread はメインスレッドから呼ぶと同期実行。defer は別スレッド経由）、§4.1 の型を実装に合わせた（検証係の指摘） (kuroko/claude-opus-5-5)
* **Update**: [DESIGN.md](../DESIGN.md) — 色付けの例外（設定画面の標準部品）を明記 (kuroko/claude-opus-5-5)
* **Creation**: [DESIGN.md](../DESIGN.md) — Apple 風の方向性と、Windows の透過窓での写し方（ほぼ不透明の素材・影は窓内）・トークン・部品 (kuroko/claude-opus-5-5)
* **Update**: [技術仕様書](architecture.md) — §3-b ペットの周りの UI（窓・フォーカス・外側クリックのフック・使うときだけ作って 60 秒で破棄・WebView2 の起動引数）、§4.2 を要件に合わせた (kuroko/claude-opus-5-5)
* **Update**: [プロダクト要求定義書](product-requirements.md) — N1 のメモリを「普段 50MB・UI 表示中 150MB（子プロセス込みの専有）」に改訂。旧値は UI の WebView2 の子プロセスを見込んでいなかった (kuroko/claude-opus-5-5)
* **Update**: [ロードマップ](roadmap.md) — Phase 1 を実施済みに (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [技術仕様書](architecture.md) — §3 の位置の保存・画面構成の変化を「合格（実機で外して付け直し）」に。W10 を K の了承のうえ実測 (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [ロードマップ](roadmap.md) — Phase 0 を W1〜W12 すべて合格に。スクショの扱いは K と決める (kuroko/claude-opus-5-5 ← human:K)
* **Update**: [OpenPets 解析](openpets-analysis.md) — 「pet-claude での扱い」列を Qt/Python 前提から Rust（Win32 layered window）に直した（評価係の指摘） (kuroko/claude-opus-5-5)
* **Update**: [ロードマップ](roadmap.md) — Phase 0 の W11 を観測値（1.6 秒以内）に、PRD §7 の受け入れ条件に未達であることを明記（評価係の指摘） (kuroko/claude-opus-5-5)
* **Update**: [技術仕様書](architecture.md) — 技術を Python + PySide6 から Rust（Win32 layered window ＋ Tauri 2）へ変更。Phase 0 の実測結果を §3 に反映 (kuroko/claude-opus-5-5)
* **Update**: [ロードマップ](roadmap.md) — Phase 0 を実施済みに。Phase 2〜3 の実装言語を Rust に合わせて修正 (kuroko/claude-opus-5-5)
* **Update**: [プロダクト要求定義書](product-requirements.md) — 受け入れ条件のテスト手段を `cargo test` に (kuroko/claude-opus-5-5)
* **Creation**: [初期要件](ideas/initial-requirements.md) — K の指示（2026-10-21）を整理。状態→動きの表を含む (kuroko/claude-fable-5-1)
* **Creation**: [OpenPets 解析](openpets-analysis.md) — 転用する設計 9 点・転用しない設計 7 点・hook 対応表・シート仕様 (kuroko/claude-fable-5-1)
* **Creation**: [プロダクト要求定義書](product-requirements.md) — 初版。未決 U1〜U3 は K 確認待ち（未承認） (kuroko/claude-fable-5-1)
* **Creation**: [技術仕様書](architecture.md) — Python 3.12 + PySide6 を採用。窓・行動・hooks・声・IPC の設計 (kuroko/claude-fable-5-1)
* **Creation**: [ロードマップ](roadmap.md) — Phase 0〜4 (kuroko/claude-fable-5-1)
* **Creation**: [Phase 0 チェックリスト](checklists/phase0-window.md) — 窓の実測項目 W1〜W12 (kuroko/claude-fable-5-1)
- 2026-10-20 extras/github/workflows: ci-node.yml を改訂（paths-ignore・concurrency・permissions・TZ・生成物の注記・本番と同じ Node）、pnpm 版 ci-node-pnpm.yml を追加、init-extras の表に反映
- 2026-10-20 CLAUDE.md: /init-extras の CI を必須にし、検証の原則に「GitHub Actions でも回す」を追記
- 2026-10-20 extras/github/workflows: 起動時 env 検証向けのダミー値の注記を ci-node / ci-node-pnpm に追加
- 2026-10-28 docs/architecture.md §6.1・§7, DESIGN.md: チャットを Claude Code と同じ環境で起こす（model/effort/権限モードの既定は設定・切替はチャット欄のメニュー・作業フォルダはホーム）。実測と常駐案は .steering/20261027-chat-same-env/brief.md (kuroko/claude-fable-5-1 ← human:K)
- 2026-10-28 docs/architecture.md §6.1, DESIGN.md: チャット欄の答えの欄を 188→264px（一番高い切替メニューと同じ）にし、答えが出ている間はメニューを開け閉めしても窓を動かさない（会話が一瞬消えるバグ）。経緯は .steering/20261028-fix-chat-menu-blink/brief.md (kuroko/claude-opus-5-5 ← human:K)
- 2026-10-28 README.md, docs/usage.md（新設）, docs/manual/（新設）: 利用者向け文書 3 点。README を実態（6 体・Phase 3〜4 実装済み・ウェイクワードは保留）に合わせて「できること→使い方→技術の詳細」に書き直し、使い方 docs/usage.md と PDF マニュアル docs/manual/pet-claude-manual.pdf（manual.html＋build-pdf.js・実機のスクリーンショット 11 枚）を新設。計画 .steering/20261028-user-docs/tasklist.md (kuroko/claude-fable-5-1 ← human:K)
