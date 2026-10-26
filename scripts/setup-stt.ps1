# 音声入力の文字起こしの準備（1 回だけ）。pet-claude 専用の Python 環境に faster-whisper を入れる。
#
# 置き場所: %LOCALAPPDATA%\pet-claude\stt\venv（pet-app の voice.rs がここの python.exe を使う）
# 版は 2026-10-22 にこの PC で動いていた faster-whisper 1.2.1 に固定する（ctranslate2 4.8.2 が一緒に入る）。
# モデル small は Hugging Face のキャッシュ（%USERPROFILE%\.cache\huggingface）に無ければここで 1 回だけ取る。
# 実行中の pet-app は取りに行かない（HF_HUB_OFFLINE=1 で起こす）。
#
# 使い方（PowerShell）: powershell -NoProfile -ExecutionPolicy Bypass -File scripts\setup-stt.ps1

$ErrorActionPreference = 'Stop'
$venv = Join-Path $env:LOCALAPPDATA 'pet-claude\stt\venv'
$python = Join-Path $venv 'Scripts\python.exe'

if (-not (Test-Path $python)) {
    Write-Host "Python の環境を作ります: $venv"
    # 3.10 は faster-whisper の wheel が揃っている版（3.13 でも動くが、手元の実績は 3.10 と 3.13 の両方）
    & py -3.10 -m venv $venv
    if ($LASTEXITCODE -ne 0) { throw 'venv を作れませんでした（py -3.10 が要ります）' }
}

& $python -m pip install --disable-pip-version-check --quiet 'faster-whisper==1.2.1'
if ($LASTEXITCODE -ne 0) { throw 'faster-whisper を入れられませんでした' }

# モデルを用意する（キャッシュにあれば何もしない）
& $python -c "from faster_whisper import WhisperModel; WhisperModel('small', device='cpu', compute_type='int8'); print('model ok')"
if ($LASTEXITCODE -ne 0) { throw 'モデル small を用意できませんでした' }

# 0.5 秒の無音で 1 回走らせて、pet-app と同じ起こし方で動くことを確かめる
$wav = Join-Path $env:TEMP 'pet-claude-stt-check.wav'
& $python -c "import wave; w=wave.open(r'$wav','wb'); w.setnchannels(1); w.setsampwidth(2); w.setframerate(16000); w.writeframes(b'\x00\x00'*8000); w.close()"
$env:HF_HUB_OFFLINE = '1'
$script = Join-Path $PSScriptRoot 'stt\transcribe.py'
& $python -I $script $wav
$code = $LASTEXITCODE
Remove-Item $wav -ErrorAction SilentlyContinue
if ($code -ne 0) { throw "文字起こしの試しに失敗しました（終了コード $code）" }
Write-Host '準備できました。設定画面の「音声入力」が「使えます」になります。'
