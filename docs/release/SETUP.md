---
type: Guide
description: pet-claude 配布 zip の初回セットアップ手順。Claude Code がこのファイルを読んで実行する前提で、1 手順＝1 コマンド＋確認の形で書く
generated: { by: kuroko/claude-fable-5-1, at: 2026-10-28T14:30:00+09:00 }
---

# pet-claude 初回セットアップ（Claude Code 用）

このファイルは **Claude Code に実行してもらう手順書**です。zip を展開したフォルダで Claude Code を開き、次のように頼んでください。

```
SETUP.md を読んで、書いてあるとおりに初回セットアップをして
```

以下は Claude Code への指示です。手順は上から順に、1 つ終えるごとに「確認」を満たしてから次へ進むこと。確認を満たせなければそこで止まり、何が起きたかを利用者に伝える。

## 前提（Claude Code が守ること）

- シェルは **PowerShell**。コマンドはこのファイルに書いた形のまま実行する。管理者権限は要らない。
- 作業フォルダは **この SETUP.md があるフォルダ**（以下 `<APP>`）。`pet-app.exe`・`pet-hook.exe`・`pets\`・`pet-claude.ico` が同じ場所にあること。
- `~/.claude/settings.json` は **`pet-app.exe --install-hooks` だけで変更する**（手で編集しない。pet-app が元のファイルを `settings.json.pet-claude-backup-<時刻>` に残してから、印付きの項目だけを足す）。
- パスワード・API キーは扱わない。利用者に入力させる場面はない。
- 手順の中で利用者に見てもらうのは「ペットが画面に出たか」の 1 点だけ。

## 0. 前提を確かめる

```powershell
[System.Environment]::OSVersion.VersionString   # Windows 10.0.22000 以上（Windows 11）
claude --version                                # Claude Code が入っている
Test-Path .\pet-app.exe; Test-Path .\pet-hook.exe; Test-Path .\pets\mame-dragon\pet.json; Test-Path .\pet-claude.ico
```

確認: Windows 11、`claude --version` が版を返す、4 つの `Test-Path` がすべて `True`。
`claude` が無ければ、先に Claude Code を入れてログインする（https://docs.claude.com/ja/docs/claude-code/setup）。

## 1. 置き場所を決める（移動しない場所へ）

hooks とショートカットは `pet-app.exe` の**絶対パス**を持つ。あとでフォルダを動かすと両方切れるので、最初に落ち着く場所へ置く。

- 推奨: `%LOCALAPPDATA%\Programs\pet-claude`（利用者のユーザー配下。管理者権限が要らない）
- ダウンロード フォルダや一時フォルダに展開してあるなら、次で移してからそのフォルダで続ける。

```powershell
$dest = Join-Path $env:LOCALAPPDATA 'Programs\pet-claude'
if ((Get-Location).Path -ne $dest) {
  New-Item -ItemType Directory -Force $dest | Out-Null
  Copy-Item -Path .\* -Destination $dest -Recurse -Force
  Set-Location $dest
}
Get-Location
```

確認: `Get-Location` が `<APP>` として使う最終の場所を指す。以降の手順はすべてこのフォルダで実行する。

## 2. Claude Code の hooks を登録する

```powershell
(.\pet-app.exe --install-hooks 2>&1 | Out-String).Trim()
```

出力例: `C:\Users\<名前>\.claude\settings.json: 登録した（元のファイル: ...pet-claude-backup-...）`

`pet-app.exe` は窓を持たないアプリなので、PowerShell は `| Out-String` を付けないと**終わりを待たずに次へ進む**（直後の確認が書き換え前のファイルを読む）。CLI の引数を渡すときは必ずこの形で呼ぶ。

確認:

```powershell
(Get-Content "$env:USERPROFILE\.claude\settings.json" -Raw | ConvertFrom-Json).hooks.PSObject.Properties.Name
Select-String -Path "$env:USERPROFILE\.claude\settings.json" -Pattern 'pet-hook.exe' | Measure-Object | Select-Object -ExpandProperty Count
```

`UserPromptSubmit`・`PreToolUse`・`PermissionRequest`・`Notification`・`Stop`・`StopFailure` が並び、`pet-hook.exe` の行が **7** ある（PreToolUse は `*` と `AskUserQuestion` の 2 組）。既に他の hooks があっても、それらは残っている。

## 3. 窓を出さずに起こす補助スクリプトを置く（自動起動の準備）

自動起動（サインイン時にペットを出す）は、コンソールの窓を一瞬も見せないために `run-hidden.vbs` を通して起こす。pet-app はこれを `%USERPROFILE%\.claude\scripts\run-hidden.vbs` に探す。

```powershell
$vbs = Join-Path $env:USERPROFILE '.claude\scripts\run-hidden.vbs'
if (-not (Test-Path $vbs)) {
  New-Item -ItemType Directory -Force (Split-Path $vbs) | Out-Null
  Copy-Item .\scripts\run-hidden.vbs $vbs
}
Test-Path $vbs
```

確認: `True`。既にあれば触らない（同じ名前の別の物を上書きしない）。

## 4. デスクトップにショートカットを置く（まめドラゴンのアイコン）

```powershell
$app = (Get-Location).Path
$lnk = Join-Path ([Environment]::GetFolderPath('Desktop')) 'pet-claude.lnk'
$s = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk)
$s.TargetPath       = Join-Path $app 'pet-app.exe'
$s.WorkingDirectory = $app
$s.IconLocation     = (Join-Path $app 'pet-claude.ico') + ',0'
$s.Description      = 'pet-claude: Claude Code のデスクトップペット'
$s.Save()
$s = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk); $s.TargetPath; $s.IconLocation
```

確認: 最後の 2 行が `<APP>\pet-app.exe` と `<APP>\pet-claude.ico,0`。デスクトップに緑のまめドラゴンのアイコン「pet-claude」が出る（アイコンが古い絵のままなら、エクスプローラーで F5）。

## 5. 起動して、ペットが出ることを確かめる

```powershell
Start-Process (Join-Path (Get-Location).Path 'pet-app.exe')
Start-Sleep 5
Get-Process pet-app -ErrorAction SilentlyContinue | Select-Object Id, Path
Get-Content "$env:LOCALAPPDATA\pet-claude\logs\pet.log" -Tail 5
```

確認: `pet-app` のプロセスがあり、`Path` が `<APP>\pet-app.exe`。ログの末尾に「既に起動しているので終了」が**無い**（あれば前から別の pet-app が動いている。トレイのアイコンを右クリック→「終了」で止めてからもう一度）。

ここで利用者に 1 つだけ聞く: **「画面の右下あたりにまめドラゴンが出ていますか？」**。出ていなければ、ログの末尾 20 行を見せて止まる（「ペットを読めない」ならフォルダ `pets\` が exe と同じ場所に無い）。

## 6. Claude Code とつながっているか確かめる

利用者がいつも使う Claude Code（ターミナルかデスクトップ アプリ）で、何か 1 つ頼んでもらう。作業の間ペットが作業中のポーズになり、終わると「できたよ」の吹き出しが出れば完了。

Claude Code の hooks は **settings.json を読み直した次のセッションから**効く。既に開いているセッションは一度閉じて開き直す。

## 7. 任意（利用者が望んだときだけ）

| したいこと | 手順 | 確認 |
|---|---|---|
| サインインしたら自動で出す | `(.\pet-app.exe --install-autostart 2>&1 \| Out-String).Trim()` | `(.\pet-app.exe --autostart-status 2>&1 \| Out-String).Trim()` が `On` を返す。設定画面「サインインしたらペットを出す」でも切り替えられる |
| 音声入力（話してチャットに送る） | `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\setup-stt.ps1`（Python 3.10 の `py -3.10` が要る。faster-whisper と small モデルを `%LOCALAPPDATA%\pet-claude\stt` に入れる。数分） | 末尾に「準備できました」。設定画面「音声入力」が「使えます」 |
| 読み上げ（声で知らせる） | AivisSpeech を `%USERPROFILE%\dev\voice\_engines\AivisSpeech\AivisSpeech-Engine\run.exe` に置く（配布 zip には入れていない。ライセンスと配布条件は AivisSpeech 側に従う） | 設定画面「読み上げ」が「エンジンは話す時に起こします」。「声で知らせる」をオンにする |

## 8. 利用者に伝えること（セットアップの最後に）

- 右クリックで設定・終了。左クリックでチャットと音声入力のバーが出る。詳しくは `docs\usage.md`、画像つきは `docs\manual\pet-claude-manual.pdf`。
- **フォルダを動かしたら** `pet-app.exe --install-hooks` をもう一度実行し、ショートカットも作り直す（手順 2 と 4）。
- 外すときは `pet-app.exe --uninstall-hooks`（hooks を外す）、`--uninstall-autostart`（自動起動を外す）、ショートカットとフォルダを消す。設定は `%LOCALAPPDATA%\pet-claude\` に残るので、要らなければそれも消す。

## 困ったとき

`docs\usage.md` の「困ったとき」の表。ログは `%LOCALAPPDATA%\pet-claude\logs\pet.log`（本文・答え・鍵は書かれない）。
