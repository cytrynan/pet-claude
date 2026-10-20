# CLAUDE_CODE_OAUTH_TOKEN を複数リポの Actions secret に登録する（人が実行する）。
# 使い方:
#   1. 別の PowerShell で `claude setup-token` を実行し、表示されたトークンをコピーする
#   2. powershell -ExecutionPolicy Bypass -File set-oauth-secret.ps1 -Repos owner/repo1,owner/repo2
#   3. 貼り付けを求められたらトークンを貼って Enter（画面には出ない）
# トークンは標準入力経由で gh に渡す（コマンドライン引数に載せない）。ファイルにも残さない。
param(
    [Parameter(Mandatory = $true)][string[]]$Repos,
    [string]$Name = "CLAUDE_CODE_OAUTH_TOKEN"
)

$secure = Read-Host -AsSecureString "トークンを貼り付けて Enter"
$bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
try {
    $plain = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr)
    if ([string]::IsNullOrWhiteSpace($plain)) { throw "トークンが空です" }
    foreach ($r in $Repos) {
        $plain | gh secret set $Name -R $r
        if ($LASTEXITCODE -eq 0) { Write-Host "OK  $r" } else { Write-Host "NG  $r（gh の出力を確認）" }
    }
} finally {
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr)
    Remove-Variable plain -ErrorAction SilentlyContinue
}
