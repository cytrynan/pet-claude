# ウェイクワードの学習用に K の声を録る（Phase 4・F20）。1 回あたり 2 秒、合図が出たら 1 回言う。
#
# 録るもの（約 2 分）:
#   1. ペットのウェイクワード（pet.json の wakeWord）を -Count 回（既定 20）
#   2. 画面に出る別の文を 5 つ（K の声の「ウェイクワードではない」例。誤反応を減らす）
#   3. 黙ったままの部屋の音を 10 秒
# 置き場所: %LOCALAPPDATA%\pet-claude\wake\data\<ペット>\k\（リポジトリには入れない。K の声なので）
# 同じフォルダに既にあれば続きの番号で足す（録り直したい時はフォルダごと消す）。
# マイクは Windows の既定の録音デバイス。録音は winmm の MCI（追加のソフトは要らない）。
#
# 使い方（PowerShell）: powershell -NoProfile -ExecutionPolicy Bypass -File scripts\wake\record-wake.ps1
#   -Pet <ペットの id>（既定 mame-dragon）-Count <回数> -Seconds <1 回の秒数>

param(
    [string]$Pet = 'mame-dragon',
    [int]$Count = 20,
    [double]$Seconds = 2.0,
    [string]$OutDir = ''
)

$ErrorActionPreference = 'Stop'

Add-Type -Namespace PetClaude -Name Mci -MemberDefinition @'
[DllImport("winmm.dll", CharSet = CharSet.Unicode)]
public static extern int mciSendStringW(string command, System.Text.StringBuilder ret, int retLen, System.IntPtr hwnd);
[DllImport("winmm.dll", CharSet = CharSet.Unicode)]
public static extern bool mciGetErrorStringW(int err, System.Text.StringBuilder buf, int len);
'@

function Invoke-Mci([string]$cmd) {
    $code = [PetClaude.Mci]::mciSendStringW($cmd, $null, 0, [IntPtr]::Zero)
    if ($code -ne 0) {
        $sb = New-Object System.Text.StringBuilder 256
        [void][PetClaude.Mci]::mciGetErrorStringW($code, $sb, 256)
        throw "MCI の失敗（$cmd）: $($sb.ToString())"
    }
}

# 16kHz・モノラル・16bit で録って保存する。保存したファイルの最大振幅を返す（ほぼ 0 ならマイクが届いていない）。
# 合図（$prompt）は録音が始まってから出す（record の直後は 0.3 秒ほど録れていないので、その分も待つ。実測 2026-10-23）
function Save-Recording([string]$path, [double]$secs, [string]$prompt) {
    Invoke-Mci 'open new type waveaudio alias petrec'
    try {
        Invoke-Mci 'set petrec time format ms bitspersample 16 channels 1 samplespersec 16000 bytespersec 32000 alignment 2'
        Invoke-Mci 'record petrec'
        Start-Sleep -Milliseconds 400
        Write-Host -NoNewline $prompt
        Start-Sleep -Milliseconds ([int]($secs * 1000))
        Invoke-Mci 'stop petrec'
        Invoke-Mci "save petrec `"$path`""
    } finally {
        [void][PetClaude.Mci]::mciSendStringW('close petrec', $null, 0, [IntPtr]::Zero)
    }
    $bytes = [System.IO.File]::ReadAllBytes($path)
    $peak = 0
    for ($i = 44; $i + 1 -lt $bytes.Length; $i += 2) {
        $v = [Math]::Abs([int][BitConverter]::ToInt16($bytes, $i))
        if ($v -gt $peak) { $peak = $v }
    }
    return $peak
}

function Next-Path([string]$dir, [string]$prefix) {
    $n = 1
    while (Test-Path (Join-Path $dir ('{0}-{1:D3}.wav' -f $prefix, $n))) { $n++ }
    return Join-Path $dir ('{0}-{1:D3}.wav' -f $prefix, $n)
}

function Countdown {
    foreach ($s in 3, 2, 1) { Write-Host -NoNewline "$s "; Start-Sleep -Milliseconds 400 }
}

$petJson = Join-Path $PSScriptRoot "..\..\assets\pets\$Pet\pet.json"
if (-not (Test-Path $petJson)) { throw "ペットが見つかりません: $petJson" }
$word = (Get-Content -Raw -Encoding UTF8 $petJson | ConvertFrom-Json).wakeWord
if (-not $word) { throw "$Pet の pet.json に wakeWord がありません" }

if (-not $OutDir) {
    $base = if ($env:PET_CLAUDE_DATA_DIR) { $env:PET_CLAUDE_DATA_DIR } else { Join-Path $env:LOCALAPPDATA 'pet-claude' }
    $OutDir = Join-Path $base "wake\data\$Pet\k"
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$others = @('今日はいい天気だね', 'お昼ご飯なに食べようかな', 'ちょっと休憩しようか', 'ビルドが通ったら教えて', 'まめに連絡してね')
$quiet = 0

Write-Host ""
Write-Host "「$word」を $Count 回録ります。「● どうぞ」が出たら 1 回だけ言ってください（$Seconds 秒）。"
Write-Host "普段ペットに呼びかける声の大きさ・速さで。少しずつ言い方を変えても大丈夫です。"
Read-Host 'Enter で始めます'
for ($i = 1; $i -le $Count; $i++) {
    Write-Host -NoNewline "[$i/$Count] "
    Countdown
    $peak = Save-Recording (Next-Path $OutDir 'word') $Seconds '● どうぞ '
    if ($peak -lt 300) { $quiet++; Write-Host "（音が小さい: $peak）" } else { Write-Host 'OK' }
}

Write-Host ""
Write-Host '次は、画面に出る文を読んでください（ウェイクワードではない例になります）。'
foreach ($t in $others) {
    Write-Host -NoNewline "「$t」 "
    Countdown
    [void](Save-Recording (Next-Path $OutDir 'other') 3.0 '● どうぞ ')
    Write-Host 'OK'
}

Write-Host ""
Write-Host '最後に 10 秒、黙ったままで部屋の音を録ります。'
Countdown
[void](Save-Recording (Next-Path $OutDir 'room') 10.0 '● 録音中 ')
Write-Host 'OK'

Write-Host ""
Write-Host "保存しました: $OutDir"
if ($quiet -gt 0) {
    Write-Host "注意: $quiet 回は音が小さく録れていました。マイクがミュートか、既定の録音デバイスが違うかもしれません。"
}
