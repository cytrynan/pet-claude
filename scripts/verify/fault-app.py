"""アプリ側の受け口の故障注入（2026-10-21 検証係の指摘 2 件の確認）。

1. 少しずつ送り続ける接続（認証前）が 1 本あっても、正常な hook がすぐ返る
2. 承認を登録したまま問い合わせを 6 秒止めると、その承認は失効する（GET が 404・カードを下げたログ）

使い方: py -3 -I scripts/verify/fault-app.py <pet-app.exe> <作業用の空フォルダ>
前提: pet-app が起動していないこと
"""

import json
import socket
import subprocess
import sys
import threading
import time
from pathlib import Path

app_exe, work = sys.argv[1], Path(sys.argv[2])
data = work / "data"
data.mkdir(parents=True, exist_ok=True)
disc_path = data / "runtime" / "ipc.json"
if disc_path.exists():
    disc_path.unlink()

# 親環境に同じ名前があっても上書きする（dict(**environ, KEY=...) は名前が重なると TypeError）
env = dict(__import__("os").environ)
env.update(PET_CLAUDE_DATA_DIR=str(data), PET_TRACE="1")
# 自動応答が残っていると放置の試験が自動承認の試験に変わる（2026-10-21 検証係の指摘）
env.pop("PET_AUTO_DECIDE", None)
app = subprocess.Popen([app_exe], env=env)
for _ in range(50):
    if disc_path.exists():
        break
    time.sleep(0.2)
disc = json.loads(disc_path.read_text(encoding="utf-8"))
host, port = disc["endpoint"].split(":")
port = int(port)


def request(method, path, body="", token=disc["token"], timeout=3.0):
    """1 往復。 (状態コード, 本文, 秒) を返す。"""
    start = time.monotonic()
    s = socket.create_connection((host, port), timeout=timeout)
    s.settimeout(timeout)
    raw = body.encode()
    head = f"{method} {path} HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {token}\r\nContent-Length: {len(raw)}\r\n\r\n"
    s.sendall(head.encode() + raw)
    resp = b""
    while True:
        chunk = s.recv(4096)
        if not chunk:
            break
        resp += chunk
    s.close()
    text = resp.decode("utf-8", "replace")
    status = int(text.split(" ")[1]) if text else 0
    return status, text.split("\r\n\r\n", 1)[-1], time.monotonic() - start


results = []

# 1. 少しずつ送る接続を開いたまま、正常な要求を出す
stop = threading.Event()


def drip():
    s = socket.create_connection((host, port))
    try:
        for b in b"GET /permission/x HTTP/1.1\r\nHost: slow\r\n":
            if stop.is_set():
                break
            s.send(bytes([b]))
            time.sleep(0.4)
    except OSError:
        pass  # アプリが期限で切った
    finally:
        s.close()


t = threading.Thread(target=drip)
t.start()
time.sleep(0.5)
ev = json.dumps({"kind": "stop", "session": "fault"})
status, _, took = request("POST", "/event", ev)
results.append(("遅い接続があっても正常な要求が返る", status == 200 and took < 1.0, f"status={status} {took:.3f}s"))
stop.set()
t.join()

# 1-b. 認証付きの要求を、2 秒の期限を過ぎてから完成させる → 受け付けない（400 か切断）
s = socket.create_connection((host, port), timeout=5)
s.settimeout(5)
late_body = json.dumps({"kind": "stop", "session": "late"}).encode()
late_head = (f"POST /event HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {disc['token']}\r\n"
             f"Content-Length: {len(late_body)}\r\n\r\n").encode()
s.sendall(late_head[:20])
for i in (1, 2):
    time.sleep(0.8)
    try:
        s.sendall(late_head[20 * i:20 * (i + 1)])
    except OSError:
        break
time.sleep(0.8)
try:
    s.sendall(late_head[60:] + late_body)
    late = s.recv(4096).decode("utf-8", "replace")
except OSError:
    late = ""
s.close()
results.append(("期限を過ぎて完成した要求は受け付けない", " 200 " not in late, late.split("\r\n", 1)[0] or "切断"))

# 2. 承認を登録し、6 秒問い合わせない
# 吹き出しは画面に出る。試験中に押されると放置にならないので、押さないよう書いておく
# （2026-10-21 の 2 回目は 3.7 秒後に「許可」が押されて FAIL した＝ログの pressed=allow）。
# 押された場合は結果に「押された」と出す
perm = json.dumps({"kind": "permission", "session": "fault", "tool": "試験中（押さないでください）",
                   "summary": "pet-claude fault test", "detail": "exact"})
status, body, _ = request("POST", "/permission", perm)
pid = json.loads(body)["id"]
status1, body1, _ = request("GET", f"/permission/{pid}")
time.sleep(6.5)
status2, body2, _ = request("GET", f"/permission/{pid}")
results.append(("問い合わせ中は pending", status1 == 200 and "pending" in body1, f"{status1} {body1}"))
results.append(("6 秒放置した承認は失効（404）", status2 == 404, f"{status2} {body2}"))

app.kill()
app.wait()
log = (data / "logs" / "pet.log").read_text(encoding="utf-8")
results.append(("放置された承認のカードを下げた", "放置された承認のカードを下げた" in log, ""))
if f"permission {pid} pressed=" in log:
    print("注意: 試験中に吹き出しのボタンが押された（この回の放置の判定は無効）")

ok = True
for name, passed, detail in results:
    ok &= passed
    print(f"{'PASS' if passed else 'FAIL'} {name} {detail}")
sys.exit(0 if ok else 1)
