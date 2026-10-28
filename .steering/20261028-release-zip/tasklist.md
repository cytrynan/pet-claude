---
type: Steering
generated: { by: kuroko/claude-fable-5-1, at: 2026-10-28T14:00:00+09:00 }
---

# リリース 1.0.0 と配布 zip（T1）

## brief
- K 指示（2026-10-28）「一旦ここでリリースバージョンとする。配布は zip ファイルで行う。そのファイルに初回セットアップの方法を claudecode 用に書いておいて（デスクトップにまめドラゴンのイラストでショートカットを配置することも込で）。完成した zip はデスクトップフォルダにおいて」。
- 版は 0.1.0（未リリース）→ **1.0.0**（Cargo.toml の workspace.package・tauri.conf.json）。CHANGELOG に 1.0.0 の節。
- 同梱物は `scripts/release.ps1` の `$Manifest` が正本: release ビルドの `pet-app.exe`・`pet-hook.exe`、`pet-claude.ico`、`pets\`（6 体。`SOURCE.md` は作者の PC のパスなので除く）、`SETUP.md`（Claude Code が読んで実行する手順）、`README.md`（同梱用）、`docs\usage.md`・`docs\pet-claude-manual.pdf`、`scripts\run-hidden.vbs`（自動起動が `%USERPROFILE%\.claude\scripts\` に探す。無い PC 向けに同梱して SETUP が置く）、`scripts\setup-stt.ps1`＋`stt\transcribe.py`、LICENSE、CHANGELOG。
- アイコン: `scripts/make-icon.py` がまめドラゴンのシートの待機 1 コマ目から `crates/pet-app/icons/icon.{png,ico}` を作る（exe・トレイ・ショートカットが同じ絵）。`gen_assets` は `target/gen-assets/` へ書くように変え、本物を上書きしない。
- 検証: release.ps1 が zip を展開し直して ①一覧の一致 ②`pet-app.exe --autostart-status` が動く ③`--install-hooks --settings <作業用>` が同梱の pet-hook.exe を 7 組書き既存の hook を残す、を機械で確かめる。加えて SETUP.md の手順 4（ショートカット）を実際に実行して `.lnk` のアイコンを目視。
- 触らない: 常駐ペット（main の debug exe）。マージ不要（main の作業ツリーで直接 commit。K 指示の作業）。

## タスク
- [x] まめドラゴンのアイコン（make-icon.py＋テスト 5 本 OK）・版 1.0.0・CHANGELOG (#137)
- [x] SETUP.md・同梱 README・release.ps1（自己検査つき。PowerShell 5.1 でもパース OK） (#138)
- [x] release ビルド（4m05s）→ zip 69.6MB・1932 ファイル → 自己検査 OK（一覧一致・--autostart-status・--install-hooks 7 組）→ SETUP の手順 0〜4 を展開した試験フォルダで実行（ショートカットは試験フォルダに作って TargetPath/IconLocation を読み返し、ico の 6 サイズを描いて目視）→ `%USERPROFILE%\Desktop\pet-claude-1.0.0-windows-x64.zip` (#139)。未: release exe の常駐起動（常駐ペットが mutex を持つので止めないと確かめられない。K の指示待ち）
- [x] gate-commit → タグ v1.0.0 → push (#140)。commit 9f5e70f・タグ v1.0.0・push 済み

## 止まった点
- SETUP の手順 2 を最初 `.\pet-app.exe --install-hooks` と素で書いて試したら、直後の確認が hooks 1 種（書き換え前）を読んだ。pet-app は GUI サブシステムなので PowerShell が終わりを待たない。`(... 2>&1 | Out-String)` の形に直し、再実行で 6 種・7 組を確認。release.ps1 の自己検査は最初からパイプ形だったので通っていた
- release.ps1 の `"exit $LASTEXITCODE: $x"` は PowerShell 5.1 のパーサが `$LASTEXITCODE:` を変数名と読んで構文エラー（pwsh は通る）。`${LASTEXITCODE}` に。規範どおり powershell.exe 側で ParseFile して見つけた
- 自動起動は `%USERPROFILE%\.claude\scripts\run-hidden.vbs`（K の規範の補助）に依存していた。配布先に無いので zip に同梱し、SETUP の手順 3 で無ければ置く（あれば触らない）。Rust 側は変えていない

## 実機（2026-10-28 14:08 K 指示「起こし直す」）
- SETUP.md の手順 1〜7 を K の PC で実行: zip を `%LOCALAPPDATA%\Programs\pet-claude` に展開 → debug の常駐（PID 67704・ExecutablePath で照合）を停止 → `--install-hooks`（pet-hook 7 組が release 側・debug 側 0）→ run-hidden.vbs は既存 → デスクトップの `pet-claude.lnk` を release exe＋`pet-claude.ico` に → `--install-autostart`（`On`）→ 起動 PID 15268（ログ `start 1.0.0`・まめドラゴン 160ms・2 画面）。`claude -p` で hooks が届き、作業中→「できたよ」の吹き出し・TTS 起動をログで確認。画面の切り出しで作業中のポーズを目視
