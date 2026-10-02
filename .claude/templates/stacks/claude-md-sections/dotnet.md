## 技術スタック

- .NET 8 SDK（`winget install Microsoft.DotNet.SDK.8`。VS が無くても CLI だけで build / test / publish が完結する）
- GUI: WinForms（`net8.0-windows`）／CLI: console
- テスト: xunit（+ NSubstitute）
- 品質ゲート: `Directory.Build.props` の `TreatWarningsAsErrors=true`（「0 警告」を受入条件として機械確認できる）

## 主要コマンド

```bash
dotnet nuget list source                       # 初回だけ: nuget.org が無ければ add source（SDK を入れただけの PC は未設定のことがある）
dotnet build <sln> -c Release                  # 0 警告・0 エラーが合格
dotnet test <sln> -c Release --filter "Category!=e2e"   # 単体（実ブラウザ・実機を使うテストは Category=e2e に分ける）
dotnet publish <csproj> -c Release -r win-x64 --self-contained true -p:PublishSingleFile=false -o dist/<配布フォルダ名>
```

## 注意点（実測から）

- **配布は self-contained の onedir を既定**にする（配布先に .NET が無くても動く。WinForms なら約 380 ファイル・約 260 MB）。
  single-file は外部ドライバを持つライブラリ（Playwright の `.playwright\` 等）が publish 先に出ないことがある。
- 非 ASCII（日本語）の `AssemblyName` は build / test / publish / 起動まで通る。版の正は csproj の `<Version>` 1 か所にする。
- テスト project は `RuntimeIdentifier=win-x64`・`SelfContained=false` を明示する（self-contained の Exe 本体を参照しても整合検査で落ちない）。
- **xunit は MTA。WinForms の Form を作るテストは STA スレッドの中で `new Form()`（`Show()` はしない）**。
- 静的な差し替え点（時計・ログ・環境）をテストで使うなら `[assembly: CollectionBehavior(DisableTestParallelization = true)]`。
- **コンパイル型なので同じツリーで 2 体の実装係が同時にビルドできない**（`obj/`・`bin/` の排他）。並行させる Phase ごとに worktree を分け、司令塔がマージする。
- PowerShell スクリプト（`build/*.ps1`）は UTF-8 **BOM 付き**で保存する（5.1 は BOM 無しの非 ASCII を CP932 で読む。Write/Edit ツールは BOM を落とすので書いた後に付け直す）。
