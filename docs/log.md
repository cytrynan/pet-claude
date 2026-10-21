# docs 更新ログ

> ドキュメント記録規約（OKF準拠。正本: `development-guidelines` スキルの `guides/recording.md`）。
> `docs/` 配下のドキュメントを作成・更新・廃止したら、同じ作業内でここに1行追記する。新しい順。
> 書式: `* **Creation|Update|Deprecation**: [タイトル](ファイル名.md) — 何をどう変えたか (アクター)`

<!-- /setup-project でドキュメントを作成したら、最初のエントリを**このコメントの下（閉じ記号の後）**に追記してください。
コメントの中に書くと Markdown の描画に出ません（実例: 「ここに追記」と読んでコメント内へ書き続け、ログ全体が見えないまま運用された）。例:
## 2026-08-01
* **Creation**: [プロダクト要求定義書](product-requirements.md) — 初版作成・ユーザー承認済み (claude/claude-sonnet-5)
-->
## 2026-10-21
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
