"""Comfy Cloud を K のログイン済み Chrome（playwright-cli attach --extension=chrome）経由で動かす。

    python -I run_comfy.py --work <作業フォルダ> tab                   # 自分のタブで cloud.comfy.org を開く（K のタブを使わない）
    python -I run_comfy.py --work <作業フォルダ> features              # 無料枠の残り
    python -I run_comfy.py --work <作業フォルダ> upload <png...>       # 姿勢画像を上げ、uploads.json に 元の名前→Cloud の名前
    python -I run_comfy.py --work <作業フォルダ> patch <workflow.json> # LoadImage を Cloud の名前に差し替え <name>.cloud.json
    python -I run_comfy.py --work <作業フォルダ> load <workflow.cloud.json>
    python -I run_comfy.py --work <作業フォルダ> queue                 # 実行して終わるまで /api/jobs を見る
    python -I run_comfy.py --work <作業フォルダ> fetch <出力フォルダ> [<workflow.cloud.json>]  # 直近のジョブの動画を回収
    python -I run_comfy.py --work <作業フォルダ> close                 # 自分のタブを閉じる

前提: 同じシェルで先に attach しておく（SKILL.md の手順 5）。playwright-cli は現在のタブに命令を送るので、
`tab` で自分のタブを開いてから使う（2026-10-27 K が同じタブを使って監視が落ちた）。

なぜこの形か（~/.agents/knowledge/dev/comfy-cloud.md）:
- Cloud は上げた画像の名前をハッシュに変えるので、workflow の LoadImage は上げた後の名前にする。
- 大きなデータ（画像の base64）はコマンド引数に載せると Windows の長さ上限に当たるので、JS をファイルに書いて
  `run-code --filename` で渡す。base64 を str.replace で埋めると中に同じ文字列が現れて壊れるので JSON 埋め込み。
- 無料枠は実行回数で数えるので、`queue` は 1 回の実行で全本を作る前提（1 本ずつ押さない）。
"""

import argparse
import base64
import json
import subprocess
import sys
import time
from pathlib import Path

SESSION = "chrome"
WORK = Path(".")


def pw(*args: str, timeout: int = 600) -> str:
    r = subprocess.run(["playwright-cli", f"-s={SESSION}", *args], capture_output=True, text=True,
                       encoding="utf-8", timeout=timeout, shell=True)
    out = (r.stdout or "").strip()
    if r.returncode != 0:
        raise SystemExit(f"playwright-cli {args[0]} failed ({r.returncode}): {r.stderr[-800:]}\n{out[-800:]}")
    return out


def run_js(js: str, timeout: int = 600) -> str:
    """JS（async page => {...} の関数）をファイル経由で実行し、戻り値（JSON 文字列）を返す。"""
    f = WORK / "_run.js"
    f.write_text(js, encoding="utf-8", newline="\n")
    out = pw("--raw", "run-code", f"--filename={f}", timeout=timeout)
    if not out:
        raise SystemExit("playwright-cli returned nothing")
    return json.loads(out) if out.startswith('"') else out


def tab() -> None:
    pw("tab-new", "https://cloud.comfy.org/")
    run_js("async page => { await page.waitForFunction(() => window.app && window.app.api, null, {timeout: 90000}); return 'ready'; }")
    print(run_js("""async page => page.evaluate(async () => {
  const f = await (await app.api.fetchApi('/features')).json();
  return JSON.stringify({free_tier_balance: f.free_tier_balance, url: location.href});
})"""))


def close() -> None:
    tabs = pw("tab-list")
    idx = next((line.split(":")[0].strip("- ") for line in tabs.splitlines() if "cloud.comfy.org" in line), None)
    if idx is None:
        print("Comfy のタブは無い")
        return
    print(pw("tab-close", idx))


def features() -> dict:
    out = run_js("""async page => page.evaluate(async () => {
  const j = await (await app.api.fetchApi('/features')).json();
  return JSON.stringify(j.free_tier_balance);
})""")
    print(out)
    return json.loads(out)


def upload(paths: list[Path]) -> dict:
    files = {p.name: base64.b64encode(p.read_bytes()).decode() for p in paths}
    js = """async page => page.evaluate(async files => {
  const out = {};
  for (const [name, b64] of Object.entries(files)) {
    const bin = Uint8Array.from(atob(b64), c => c.charCodeAt(0));
    const fd = new FormData();
    fd.append('image', new Blob([bin], {type: 'image/png'}), name);
    fd.append('overwrite', 'true');
    fd.append('type', 'input');
    const res = await app.api.fetchApi('/upload/image', {method: 'POST', body: fd});
    const j = await res.json();
    out[name] = {status: res.status, name: j.name};
  }
  return JSON.stringify(out);
}, %s)""" % json.dumps(files)
    result = json.loads(run_js(js))
    uploads = WORK / "uploads.json"
    known = json.loads(uploads.read_text(encoding="utf-8")) if uploads.exists() else {}
    for k, v in result.items():
        if v.get("status") != 200 or not v.get("name"):
            raise SystemExit(f"upload failed: {k} -> {v}")
        known[k] = v["name"]
    uploads.write_text(json.dumps(known, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")
    print(json.dumps(result, indent=1, ensure_ascii=False))
    return known


def patch(workflow: Path) -> Path:
    known = json.loads((WORK / "uploads.json").read_text(encoding="utf-8"))
    wf = json.loads(workflow.read_text(encoding="utf-8"))
    for n in wf["nodes"]:
        if n["type"] == "LoadImage":
            orig = n["widgets_values"][0]
            if orig not in known:
                raise SystemExit(f"not uploaded: {orig}")
            n["widgets_values"][0] = known[orig]
            print(f"LoadImage {n['id']}: {orig} -> {known[orig]}")
    out = workflow.with_suffix(".cloud.json")
    out.write_text(json.dumps(wf, ensure_ascii=False, indent=1), encoding="utf-8", newline="\n")
    print(f"ok {out}")
    return out


def load(workflow: Path) -> None:
    js = """async page => page.evaluate(async wf => {
  await app.loadGraphData(wf);
  const nodes = app.graph._nodes.map(n => n.type);
  return JSON.stringify({nodes: nodes.length, loadImage: nodes.filter(t => t === 'LoadImage').length,
    save: nodes.filter(t => t === 'SaveVideo').length});
}, %s)""" % workflow.read_text(encoding="utf-8")
    print(run_js(js))


def queue(poll: int = 10, limit_s: int = 1800) -> dict:
    before = features()
    print(run_js("""async page => {
  await page.keyboard.press('Escape');
  await page.getByTestId('queue-button').click();
  await page.waitForTimeout(3000);
  return page.evaluate(async () => JSON.stringify(await (await app.api.fetchApi('/jobs?limit=1')).json()));
}"""))
    t0 = time.time()
    while time.time() - t0 < limit_s:
        time.sleep(poll)
        st = json.loads(run_js("""async page => page.evaluate(async () => {
  const j = await (await app.api.fetchApi('/jobs?limit=1')).json();
  const job = (j.jobs || [])[0] || {};
  return JSON.stringify({id: job.id, status: job.status, outputs_count: job.outputs_count, error: job.error_message || job.error});
})"""))
        print(f"{int(time.time() - t0):4d}s {st}")
        if st.get("status") in ("completed", "failed", "error", "cancelled"):
            print(f"free tier: {before} -> {features()}")
            return st
    raise SystemExit("timeout")


def fetch(out_dir: Path, workflow: Path | None) -> None:
    """直近のジョブの出力（動画）を history_v2 から探し、/api/view で回収する。
    workflow（.cloud.json）を渡すと SaveVideo の接頭辞（video/<pet>_stop）で名前を付ける。"""
    names = {}
    if workflow is not None:
        for n in json.loads(workflow.read_text(encoding="utf-8"))["nodes"]:
            if n["type"] == "SaveVideo":
                names[str(n["id"])] = n["widgets_values"][0].split("/")[-1]
    info = json.loads(run_js("""async page => page.evaluate(async () => {
  const h = await (await app.api.fetchApi('/history_v2?max_items=1')).json();
  const items = Array.isArray(h.history) ? h.history : Object.values(h.history || h);
  const item = items[0] || {};
  const files = [];
  for (const [nid, o] of Object.entries(item.outputs || {}))
    for (const key of Object.keys(o))
      for (const f of (o[key] || []))
        if (f && f.filename) files.push({node: nid, filename: f.filename, subfolder: f.subfolder || '', type: f.type || 'output'});
  return JSON.stringify({id: item.prompt_id || item.id, files});
})"""))
    out_dir.mkdir(parents=True, exist_ok=True)
    for f in info["files"]:
        if not f["filename"].endswith(".mp4"):
            continue
        r = json.loads(run_js("""async page => page.evaluate(async q => {
  const res = await app.api.fetchApi('/view?' + q);
  const buf = new Uint8Array(await res.arrayBuffer());
  let s = '';
  for (let i = 0; i < buf.length; i += 0x8000) s += String.fromCharCode.apply(null, buf.subarray(i, i + 0x8000));
  return JSON.stringify({status: res.status, size: buf.length, b64: btoa(s)});
}, %s)""" % json.dumps(f"filename={f['filename']}&type={f['type']}&subfolder={f['subfolder']}")))
        if r["status"] != 200:
            raise SystemExit(f"view failed {f}: {r['status']}")
        dst = out_dir / (names[f["node"]] + ".mp4" if f["node"] in names else f["filename"])
        dst.write_bytes(base64.b64decode(r["b64"]))
        print(f"{f['node']} {dst} {r['size']} bytes")


def main() -> None:
    global WORK
    ap = argparse.ArgumentParser()
    ap.add_argument("--work", type=Path, default=Path("."), help="uploads.json と _run.js を置く作業フォルダ")
    ap.add_argument("cmd", choices=["tab", "close", "features", "upload", "patch", "load", "queue", "fetch"])
    ap.add_argument("args", nargs="*")
    a = ap.parse_args()
    WORK = a.work
    WORK.mkdir(parents=True, exist_ok=True)
    if a.cmd == "tab":
        tab()
    elif a.cmd == "close":
        close()
    elif a.cmd == "features":
        features()
    elif a.cmd == "upload":
        upload([Path(x) for x in a.args])
    elif a.cmd == "patch":
        patch(Path(a.args[0]))
    elif a.cmd == "load":
        load(Path(a.args[0]))
    elif a.cmd == "queue":
        queue()
    elif a.cmd == "fetch":
        fetch(Path(a.args[0]), Path(a.args[1]) if len(a.args) > 1 else None)


if __name__ == "__main__":
    main()
