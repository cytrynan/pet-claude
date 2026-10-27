"""Comfy Cloud を K のログイン済み Chrome（playwright-cli attach --extension=chrome）経由で動かす。

    python -I run_comfy.py upload <png...>            # 姿勢画像を上げ、uploads.json に 元の名前→Cloud の名前 を記録
    python -I run_comfy.py patch <workflow.json>       # LoadImage の名前を Cloud の名前に差し替えて <name>.cloud.json を書く
    python -I run_comfy.py load <workflow.cloud.json>  # app.loadGraphData で読み込む
    python -I run_comfy.py queue                       # 実行ボタンを押し、終わるまで /api/jobs を見る
    python -I run_comfy.py fetch <出力フォルダ> [<workflow.cloud.json>]  # 直近のジョブの動画を回収（workflow を渡すと接頭辞の名前）
    python -I run_comfy.py features                    # 無料枠の残り

なぜこの形か（`~/.agents/knowledge/dev/comfy-cloud.md`）:
- Cloud は上げた画像の名前をハッシュに変えるので、workflow の LoadImage は上げた後の名前にする。
- 大きなデータ（画像の base64）はコマンド引数に載せると Windows の長さ上限に当たるので、
  JS をファイルに書いて `run-code --filename` で渡す。base64 を str.replace で埋めると
  中に同じ文字列が現れて壊れるので、JSON 埋め込みにする。
- 無料枠は実行回数で数えるので、`queue` は 1 回の実行で全本を作る前提（1 本ずつ押さない）。
"""

import base64
import json
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
SESSION = "chrome"
UPLOADS = HERE / "uploads.json"
# Welcome タブの URL にトークンが載るので、ログに出すときは伏せる
TOKEN_RE = "token="


def run_js(js: str, timeout: int = 600) -> str:
    """JS（async page => {...} の関数）をファイル経由で実行し、戻り値（JSON 文字列）を返す。"""
    f = HERE / "_run.js"
    f.write_text(js, encoding="utf-8", newline="\n")
    r = subprocess.run(["playwright-cli", f"-s={SESSION}", "--raw", "run-code", f"--filename={f}"],
                       capture_output=True, text=True, encoding="utf-8", timeout=timeout, shell=True)
    out = r.stdout.strip()
    if r.returncode != 0 or not out:
        raise SystemExit(f"playwright-cli failed ({r.returncode}): {r.stderr[-800:]}\n{out[-800:]}")
    return json.loads(out) if out.startswith('"') else out


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
    out[name] = {status: res.status, name: j.name, subfolder: j.subfolder, type: j.type};
  }
  return JSON.stringify(out);
}, %s)""" % json.dumps(files)
    result = json.loads(run_js(js))
    known = json.loads(UPLOADS.read_text(encoding="utf-8")) if UPLOADS.exists() else {}
    for k, v in result.items():
        if v.get("status") != 200 or not v.get("name"):
            raise SystemExit(f"upload failed: {k} -> {v}")
        known[k] = v["name"]
    UPLOADS.write_text(json.dumps(known, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")
    print(json.dumps(result, indent=1, ensure_ascii=False))
    return known


def patch(workflow: Path) -> Path:
    known = json.loads(UPLOADS.read_text(encoding="utf-8"))
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
    wf = workflow.read_text(encoding="utf-8")
    js = """async page => page.evaluate(async wf => {
  await app.loadGraphData(wf);
  const nodes = app.graph._nodes.map(n => n.type);
  return JSON.stringify({nodes: nodes.length, loadImage: nodes.filter(t => t === 'LoadImage').length,
    save: nodes.filter(t => t === 'SaveVideo').length});
}, %s)""" % wf
    print(run_js(js))


def features() -> dict:
    js = """async page => page.evaluate(async () => {
  const j = await (await app.api.fetchApi('/features')).json();
  return JSON.stringify(j.free_tier_balance);
})"""
    out = run_js(js)
    print(out)
    return json.loads(out)


def queue(poll: int = 10, limit_s: int = 1200) -> dict:
    before = features()
    js = """async page => {
  await page.keyboard.press('Escape');
  await page.getByTestId('queue-button').click();
  await page.waitForTimeout(3000);
  return page.evaluate(async () => {
    const j = await (await app.api.fetchApi('/jobs?limit=1')).json();
    return JSON.stringify(j);
  });
}"""
    print(run_js(js))
    t0 = time.time()
    while time.time() - t0 < limit_s:
        time.sleep(poll)
        js = """async page => page.evaluate(async () => {
  const j = await (await app.api.fetchApi('/jobs?limit=1')).json();
  const job = (j.jobs || [])[0] || {};
  return JSON.stringify({id: job.id, status: job.status, outputs_count: job.outputs_count,
    create_time: job.create_time, execution_time: job.execution_time, error: job.error_message || job.error});
})"""
        st = json.loads(run_js(js))
        print(f"{int(time.time() - t0):4d}s {st}")
        if st.get("status") in ("completed", "failed", "error", "cancelled"):
            after = features()
            print(f"free tier: {before} -> {after}")
            return st
    raise SystemExit("timeout")


def fetch(out_dir: Path, workflow: Path | None = None) -> None:
    """直近のジョブの出力（動画）を history_v2 から探し、/api/view で回収する。
    workflow（.cloud.json）を渡すと、SaveVideo ノードの接頭辞（video/rinrin_stop）で名前を付ける。"""
    names = {}
    if workflow is not None:
        for n in json.loads(workflow.read_text(encoding="utf-8"))["nodes"]:
            if n["type"] == "SaveVideo":
                names[str(n["id"])] = n["widgets_values"][0].split("/")[-1]
    js = """async page => page.evaluate(async () => {
  const h = await (await app.api.fetchApi('/history_v2?max_items=1')).json();
  const items = Array.isArray(h.history) ? h.history : Object.values(h.history || h);
  const item = items[0] || {};
  const files = [];
  for (const [nid, o] of Object.entries(item.outputs || {})) {
    for (const key of Object.keys(o)) {
      for (const f of (o[key] || [])) {
        if (f && f.filename) files.push({node: nid, filename: f.filename, subfolder: f.subfolder || '', type: f.type || 'output'});
      }
    }
  }
  return JSON.stringify({id: item.prompt_id || item.id, status: item.status && item.status.status_str, files, keys: Object.keys(item)});
})"""
    info = json.loads(run_js(js))
    print(json.dumps(info, indent=1))
    out_dir.mkdir(parents=True, exist_ok=True)
    # 出力ノード（SaveVideo）の接頭辞と対応付けるため、workflow 側の順番を使わず
    # ファイル名で保存し、呼び出し側で対応表を作る
    for f in info["files"]:
        if not f["filename"].endswith(".mp4"):
            continue
        js = """async page => page.evaluate(async q => {
  const res = await app.api.fetchApi('/view?' + q);
  const buf = new Uint8Array(await res.arrayBuffer());
  let s = '';
  for (let i = 0; i < buf.length; i += 0x8000) s += String.fromCharCode.apply(null, buf.subarray(i, i + 0x8000));
  return JSON.stringify({status: res.status, size: buf.length, b64: btoa(s)});
}, %s)""" % json.dumps(f"filename={f['filename']}&type={f['type']}&subfolder={f['subfolder']}")
        r = json.loads(run_js(js))
        if r["status"] != 200:
            raise SystemExit(f"view failed {f}: {r['status']}")
        dst = out_dir / (names[f["node"]] + ".mp4" if f["node"] in names else f["filename"])
        dst.write_bytes(base64.b64decode(r["b64"]))
        print(f"{f['node']} {dst} {r['size']} bytes")


def main(argv: list[str]) -> None:
    cmd, args = argv[0], argv[1:]
    if cmd == "upload":
        upload([Path(a) for a in args])
    elif cmd == "patch":
        patch(Path(args[0]))
    elif cmd == "load":
        load(Path(args[0]))
    elif cmd == "queue":
        queue()
    elif cmd == "fetch":
        fetch(Path(args[0]), Path(args[1]) if len(args) > 1 else None)
    elif cmd == "features":
        features()
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])
