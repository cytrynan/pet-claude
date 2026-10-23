"""読み上げと音声入力の故障注入（Phase 3 後半の V2・V5・V7〜V10・S11。`.steering/20261022-phase3-voice/requirements.md`）。

偽の AivisSpeech（このスクリプトが 127.0.0.1 の空いたポートで立てる HTTP）、偽の文字起こし（fake_stt.py）、
偽の claude（`cargo build -p pet-app --example fake_claude`）に差し替え、マイクの代わりに WAV を流し込む
（`PET_TEST_VOICE_WAV`）。チャットの故障注入と同じく別のミューテックス名で起動するので、普段のペットは止めなくてよい。
偽のエンジンが返すのは無音の WAV なので、鳴っても聞こえない。

使い方: py -3 -I scripts/verify/voice-fault.py <pet-app.exe> <fake_claude.exe> <作業用の空フォルダ> [ケース名,...]
"""

import http.server
import json
import math
import os
import socketserver
import struct
import subprocess
import sys
import threading
import time
from pathlib import Path

app_exe, fake_claude, work = sys.argv[1], sys.argv[2], Path(sys.argv[3])
only = set(sys.argv[4].split(",")) if len(sys.argv) > 4 else None
FAKE_STT = Path(__file__).with_name("fake_stt.py")
results = []
skipping = False
stt_alive_before_kill = []


def wav(samples, rate=16000):
    data = struct.pack(f"<{len(samples)}h", *samples)
    return (b"RIFF" + struct.pack("<I", 36 + len(data)) + b"WAVE" + b"fmt "
            + struct.pack("<IHHIIHH", 16, 1, 1, rate, rate * 2, 2, 16) + b"data"
            + struct.pack("<I", len(data)) + data)


def tone(ms, amp=3000):
    n = ms * 16
    return [int(amp * math.sin(2 * math.pi * 220 * i / 16000)) for i in range(n)]


def silence(ms):
    return [0] * (ms * 16)


def noise_floor(ms):
    # 本物のマイクは完全な 0 にならない（全部 0 は「マイクを止められている」扱いになる）
    return [(i * 37 % 21) - 10 for i in range(ms * 16)]


SPEECH = work / "speech.wav"     # 静か 0.4 秒 → 話す 1.2 秒 → 静か 1.5 秒（話し終えて Done）
LONG = work / "long.wav"         # 話し続ける 10 秒（もう一度押して止める）
QUIET = work / "quiet.wav"       # 静か 6 秒（NoSpeech）
ZEROS = work / "zeros.wav"       # 全部 0 の 6 秒（マイクを止められている）
MUTED = work / "muted.wav"       # -1〜1 だけの 6 秒（2026-10-22 K の PC のミュートのマイクと同じ形）


class Engine(http.server.BaseHTTPRequestHandler):
    mode = "ok"
    hits = []

    def log_message(self, *a):
        pass

    def reply(self, code, body, ctype="application/json"):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        Engine.hits.append(self.path.split("?")[0])
        if self.path == "/version":
            self.reply(200, b'"1.0.0"')
        elif self.path == "/speakers":
            self.reply(200, json.dumps([{"name": "天深シノ", "styles": [
                {"name": "ささやき", "id": 9}, {"name": "ノーマル", "id": 1063997408}]}]).encode())
        else:
            self.reply(404, b"")

    def do_POST(self):
        n = int(self.headers.get("Content-Length") or 0)
        self.rfile.read(n)
        Engine.hits.append(self.path.split("?")[0])
        if self.path.startswith("/audio_query"):
            ok = "speaker=1063997408" in self.path
            self.reply(200 if ok else 422, b'{"accent_phrases":[]}')
        elif self.path.startswith("/synthesis"):
            if Engine.mode == "slow":
                time.sleep(40)
            if Engine.mode == "drip":
                # 頭だけ返し、本文を 5 秒ごとに 1 バイト（読むたびの時間切れには当たらない）
                self.send_response(200)
                self.send_header("Content-Type", "audio/wav")
                self.send_header("Content-Length", "100000")
                self.end_headers()
                for _ in range(20):
                    try:
                        self.wfile.write(b"R")
                        self.wfile.flush()
                    except OSError:
                        return
                    time.sleep(5)
                return
            if Engine.mode == "long":
                # 4 秒の無音（話している途中でオフにする試験。V4）
                body = wav(silence(4000), 24000)
            elif Engine.mode == "rate0":
                # 頭は PCM16 だがサンプルレートが 0（検証係 2 巡目の例）
                body = wav(silence(200), 0)
            else:
                body = b"RIFF garbage" if Engine.mode == "broken" else wav(silence(200), 24000)
            self.reply(200, body, "audio/wav")
        else:
            self.reply(404, b"")


def start_engine():
    srv = socketserver.ThreadingTCPServer(("127.0.0.1", 0), Engine)
    srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv


def alive(pid):
    out = subprocess.run(["tasklist", "/FI", f"PID eq {pid}", "/NH", "/FO", "CSV"],
                         capture_output=True, text=True).stdout
    return f'"{pid}"' in out


def stt_log(data):
    p = data / "voice" / "fake-stt.txt"
    return p.read_text(encoding="utf-8").splitlines() if p.exists() else []


def stt_pids(data):
    return sorted({int(x.split()[0]) for x in stt_log(data)})


def stt_got_path(data):
    return [x for x in stt_log(data) if " exists=" in x]


def run_case(name, steps, *, engine="ok", tts=True, mic=None, stt="ok", stt_exe=None,
             stt_timeout=None, wait_for=None, max_secs=30, kill_after=None, addr=None, extra=None):
    """pet-app を 1 回起動し、`wait_for` の文がログに出るまで待って止める。(ログ, データのフォルダ, 生きているか)"""
    global skipping
    data = work / name
    skipping = only is not None and name not in only
    if skipping:
        return "", data, True
    data.mkdir(parents=True, exist_ok=True)
    (data / "state.json").write_text(json.dumps({"tts": tts}), encoding="utf-8")
    Engine.mode, Engine.hits = engine, []
    env = dict(os.environ)
    env.pop("PET_AUTO_DECIDE", None)
    env.update(PET_CLAUDE_DATA_DIR=str(data), PET_CLAUDE_EXE=fake_claude, FAKE_MODE="ok",
               PET_TEST_CHAT=steps, PET_TEST_AIVIS_ADDR=addr or ENGINE_ADDR, PET_TEST_AIVIS_EXE="",
               PET_TEST_STT_EXE=stt_exe if stt_exe is not None else sys.executable,
               PET_TEST_STT_SCRIPT=str(FAKE_STT), FAKE_STT=stt)
    if mic:
        env["PET_TEST_VOICE_WAV"] = str(mic)
    if stt_timeout:
        env["PET_STT_TIMEOUT_SECS"] = str(stt_timeout)
    env.update(extra or {})
    app = subprocess.Popen([app_exe], env=env)
    log_path = data / "logs" / "pet.log"
    started = time.monotonic()
    log = ""
    while time.monotonic() - started < max_secs:
        log = log_path.read_text(encoding="utf-8") if log_path.exists() else ""
        if kill_after and kill_after in log and len(stt_pids(data)) >= 2:
            time.sleep(0.8)  # 孫が起きるのを待つ
            subprocess.run(["taskkill", "/F", "/PID", str(app.pid)], capture_output=True)
            app.wait(timeout=10)
            time.sleep(1.0)
            break
        if wait_for and log.count(wait_for[0]) >= wait_for[1]:
            time.sleep(0.5)
            break
        time.sleep(0.2)
    app_alive = app.poll() is None
    # アプリを落とす前に子・孫が残っていないかを見る（落とすと Job が閉じて消えるので、後で見ると
    # 時間切れで止められなかった退行を見逃す。2026-10-24 検証係の指摘）
    global stt_alive_before_kill
    stt_alive_before_kill = [p for p in stt_pids(data) if alive(p)]
    if app_alive:
        app.kill()
        app.wait(timeout=10)
    log = log_path.read_text(encoding="utf-8") if log_path.exists() else ""
    return log, data, app_alive


def check(name, ok, detail):
    if skipping:
        return
    results.append((name, ok, detail))
    print(f"{'PASS' if ok else 'FAIL'} {name}: {detail}")


def wavs_left(data):
    d = data / "voice"
    return sorted(p.name for p in d.glob("*.wav")) if d.exists() else []


work.mkdir(parents=True, exist_ok=True)
SPEECH.write_bytes(wav(noise_floor(400) + tone(1200) + noise_floor(1500)))
LONG.write_bytes(wav(noise_floor(400) + tone(10000)))
QUIET.write_bytes(wav(noise_floor(6000)))
ZEROS.write_bytes(wav(silence(6000)))
MUTED.write_bytes(wav([(i % 3) - 1 for i in range(96000)]))
srv = start_engine()
ENGINE_ADDR = f"127.0.0.1:{srv.server_address[1]}"

# --- 読み上げ ---
log, data, alive_ = run_case("tts-ok", "send:やあ", wait_for=("tts: ", 1))
check("読み上げ: 答えを合成して鳴らす", alive_ and "1 文目を" in log and "話し終えた" in log
      and "鳴らせなかった" not in log and "/synthesis" in Engine.hits
      and "/speakers" in Engine.hits, f"hits={Engine.hits}")
check("N4 ログに読んだ文が無い", "受け取ったよ" not in log, "ログを grep")

log, data, alive_ = run_case("tts-down", "send:やあ", addr="127.0.0.1:9",
                             wait_for=("tts: 話せなかった", 1))
check("V2 エンジンが無い（落ちず黙る）", alive_ and "話せなかった（エンジンが見つからない）" in log,
      log.splitlines()[-1] if log else "")

log, data, alive_ = run_case("tts-broken", "send:やあ", engine="broken", wait_for=("tts: 話せなかった", 1))
check("V2 壊れた WAV は鳴らさない", alive_ and "エンジンが返した音を読めない" in log, f"hits={Engine.hits}")

log, data, alive_ = run_case("tts-rate0", "send:やあ", engine="rate0", wait_for=("tts: 話せなかった", 1))
check("V2 サンプルレート 0 の WAV は鳴らさず、待ち続けない", alive_ and "サンプルレートがおかしい" in log, "")

log, data, alive_ = run_case("tts-slow", "send:一|wait:3000|send:二", engine="slow",
                             wait_for=("chat answer in", 2), max_secs=20)
check("V2 エンジンが遅くてもチャットは進む", alive_ and log.count("chat answer in") == 2,
      f"answers={log.count('chat answer in')}")

log, data, alive_ = run_case("tts-drip", "send:やあ", engine="drip", wait_for=("tts: 話せなかった", 1),
                             max_secs=60)
check("V2 少しずつ返すエンジンでも期限（30 秒）で諦める", alive_ and "応答が遅すぎる" in log,
      [x[24:] for x in log.splitlines() if "tts:" in x][-1:])

# V4: 話している途中で「声で知らせる」をオフにすると止まる（4 秒の音を 1 秒ほどで止める）
log, data, alive_ = run_case("tts-off-mid", "send:やあ|wait:2500|tts:off|wait:1500", engine="long",
                             wait_for=("test chat: never", 1), max_secs=12)
stopped = [x for x in log.splitlines() if "途中で止めた" in x]
secs = float(stopped[-1].split("（")[-1].rstrip("s）")) if stopped else None
check("V4 話している途中でオフにすると止まる", alive_ and "tts: off" in log and secs is not None and secs < 3.5
      and "話し終えた" not in log, f"止めた={secs}s（音は 4 秒）")

# エンジンを起こす時に、その孫まで Job に入る（止めて起こしてから Job に入れる）。偽のエンジン＝偽の claude の
# slow（孫を起こして眠る）を run.exe という名前で置き、チャット欄を開いて先に起こさせ、アプリを落とす
eng_dir = work / "fake-engine"
eng_dir.mkdir(parents=True, exist_ok=True)
(eng_dir / "run.exe").write_bytes(Path(fake_claude).read_bytes())
(eng_dir / "fake-pids.txt").unlink(missing_ok=True)
log, data, alive_ = run_case("tts-engine-job", "ui:chat|wait:4000", addr="127.0.0.1:9",
                             extra={"PET_TEST_AIVIS_EXE": str(eng_dir / "run.exe"), "FAKE_MODE": "slow"},
                             wait_for=("test chat: never", 1), max_secs=7)
time.sleep(1.5)
eng_pids = [int(x.split()[0]) for x in (eng_dir / "fake-pids.txt").read_text().splitlines()] \
    if (eng_dir / "fake-pids.txt").exists() else []
check("エンジンとその孫は Job に入り、アプリが落ちると残らない", "tts: engine started" in log
      and len(eng_pids) >= 2 and not [p for p in eng_pids if alive(p)], f"pids={eng_pids}")

log, data, alive_ = run_case("tts-off", "send:やあ", tts=False, wait_for=("chat answer in", 1))
time.sleep(1.0)
check("V4 声がオフなら話さない", alive_ and "/synthesis" not in Engine.hits and "tts: " not in log,
      f"hits={Engine.hits}")

# --- 音声入力 ---
log, data, alive_ = run_case("voice-ok", "ui:voice", mic=SPEECH, wait_for=("chat answer in", 1))
lines = stt_log(data)
check("S11 録音→文字起こし→チャット→答え", alive_ and "字をチャットへ" in log and "chat send" in log
      and "chat answer in" in log, f"stt={len(lines)}")
check("V9 文字起こしには WAV が渡り、終わったら消える", any("exists=True" in x for x in lines)
      and not wavs_left(data), f"残り={wavs_left(data)}")

check("V5 話し終えて黙ると止まる（2.5 秒前後）", "voice: recorded 2." in log or "voice: recorded 3." in log,
      [x for x in log.splitlines() if "recorded" in x][:1])

log, data, alive_ = run_case("voice-stop", "ui:voice|wait:1500|ui:voice", mic=LONG,
                             wait_for=("chat answer in", 1))
check("V5 もう一度押すと止めて送る", "voice: stop requested" in log and "voice: recorded 1." in log
      and "chat answer in" in log, [x for x in log.splitlines() if "recorded" in x][:1])

log, data, alive_ = run_case("voice-quiet", "ui:voice", mic=QUIET, wait_for=("voice: failed", 1))
check("V5 5 秒話さなければ取りやめ", alive_ and "voice: failed: 聞き取れなかったよ" in log
      and not stt_got_path(data), f"stt={stt_log(data)}")
# 子を録音の前に起こしている証拠: 取りやめた録音でも子は起きていて、パスを受け取らずに終わる（後から起こす作りなら子はいない）
check("文字起こしの子は録音の前に起き、取りやめならパスを受けずに終わる",
      any(x.endswith(" no-path") for x in stt_log(data)), f"stt={stt_log(data)}")

log, data, alive_ = run_case("voice-zeros", "ui:voice", mic=ZEROS, wait_for=("voice: failed", 1))
check("V8 全部 0（マイクを止められている）", "マイクを使えないよ" in log, "")

log, data, alive_ = run_case("voice-muted", "ui:voice", mic=MUTED, wait_for=("voice: failed", 1))
check("V8 ほぼ無音（ミュートのマイクの形・最大振幅 1）", "マイクを使えないよ" in log and "peak=1" in log, "")

log, data, alive_ = run_case("voice-halluc", "ui:voice", mic=SPEECH, stt="hallucination",
                             wait_for=("voice: failed", 1))
check("V7 幻聴は送らない", "聞き取れなかったよ" in log and "chat send" not in log, "")

log, data, alive_ = run_case("voice-empty", "ui:voice", mic=SPEECH, stt="empty", wait_for=("voice: failed", 1))
check("V7 空の結果は送らない", "聞き取れなかったよ" in log and "chat send" not in log, "")

log, data, alive_ = run_case("voice-garbage", "ui:voice", mic=SPEECH, stt="garbage",
                             wait_for=("voice: failed", 1))
check("V8 壊れた出力", alive_ and "文字起こしがうまくいかなかったよ" in log and not wavs_left(data), "")

log, data, alive_ = run_case("voice-nostt", "ui:voice", mic=SPEECH, stt_exe="", wait_for=("voice: failed", 1))
check("V8 文字起こしの道具が無い", alive_ and "音声入力の準備が要るよ" in log, "")

log, data, alive_ = run_case("voice-timeout", "ui:voice", mic=SPEECH, stt="slow", stt_timeout=3,
                             wait_for=("voice: failed", 1), max_secs=20)
time.sleep(1.0)
pids = stt_pids(data)
left = [p for p in pids if alive(p)]
check("V8 時間切れ", alive_ and "文字起こしが終わらなかったよ" in log and not wavs_left(data), "")
check("V10 時間切れで子と孫が残らない（アプリを落とす前に）", len(pids) >= 2 and not left
      and not stt_alive_before_kill, f"pids={pids} 残り={left} 落とす前に生きていた={stt_alive_before_kill}")

log, data, alive_ = run_case("voice-crash", "ui:voice", mic=SPEECH, stt="slow", kill_after="voice: recorded")
time.sleep(2.0)
pids = stt_pids(data)
left = [p for p in pids if alive(p)]
check("V10 アプリが落ちても子と孫が残らない", len(pids) >= 2 and not left, f"pids={pids} 残り={left}")

(work / "voice-leftover" / "voice").mkdir(parents=True, exist_ok=True)
(work / "voice-leftover" / "voice" / "rec-99.wav").write_bytes(b"x")
log, data, alive_ = run_case("voice-leftover", "wait:500", wait_for=("start ", 1))
check("V9 起動時に前回の WAV を消す", not wavs_left(data), f"残り={wavs_left(data)}")

# 日本語の名前のフォルダでも、文字起こしに WAV のパスが化けずに届く（標準入力は UTF-8 で読む）
log, data, alive_ = run_case("voice-日本語", "ui:voice", mic=SPEECH, wait_for=("chat answer in", 1))
check("日本語のフォルダ名でも WAV のパスが届く", any("exists=True" in x for x in stt_log(data))
      and "chat answer in" in log, stt_got_path(data)[:1])

log, data, alive_ = run_case("voice-busy", "send:やあ|ui:voice", mic=SPEECH, wait_for=("chat answer in", 1))
check("チャットの返答待ちの間は録音を始めない", "voice: start" not in log, "")

srv.shutdown()
fails = [r for r in results if not r[1]]
print(f"\n{len(results) - len(fails)}/{len(results)} PASS")
sys.exit(1 if fails else 0)
