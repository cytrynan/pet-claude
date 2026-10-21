---
type: Steering
description: Phase 2 の検証の証拠（テストの固定ハッシュ・実行結果・E2E）
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-21T12:00:00+09:00 }
---

# Phase 2 の証拠

## テスト先行: 受け入れたテストの固定ハッシュ（実装係の起動前に記録）

| ファイル | テスト数 | sha256 |
|---|---|---|
| `crates/pet-core/tests/hook.rs` | 44 | `03d7d330bf466ecdcfb01a671df72dc40a838dee58afc6c31faf7e0e012650e7` |
| `crates/pet-core/tests/ipc.rs` | 36 | `ddaff3b4faf2d138b543951c6d9451db3a3105436817f9c3a6216607a83160c8` |

- テスト作成係: Opus（requirements.md だけを渡した。実装は見せていない）
- questions.md Q1（単独の非 ASCII 空白も半角空白にする＝解釈 A）は司令塔が A で確定。requirements.md 3 節の「連続する空白（`char::is_whitespace`）を 1 つの半角空白に」は長さ 1 の並びも含むと読める（`split_whitespace().join(" ")` と同じ）
- 実装後、同じハッシュであることを確かめてから受け入れる

## 実装後（2026-10-21 13:30 時点）

- テストのハッシュ: 実装後も上の値と同じ（実装係と司令塔の 2 回確認）
- ゲート（`scripts/gate.sh`）: exit 0。pet-app 9・pet-core 単体 33・tests/hook.rs 44・tests/ipc.rs 36・pet-hook 故障注入 9
- ミューテーション（cargo-mutants 27.1.0、hook.rs と ipc.rs）: 92 件中 86 検出・2 見逃し・4 コンパイル不可 → 単体テスト 2 本を足して見逃しの 2 関数は 16/16 検出
- E2E（`scripts/verify/e2e-hooks.sh`、`claude -p --model haiku`、2.1.305）: allow はファイルができる・deny と ask はできない、の 3 通り PASS。拒否の message が Claude の返答に出た
- アプリ側の故障注入（`scripts/verify/fault-app.py`）: 遅い接続・期限後に完成した要求・問い合わせ中は pending・6 秒放置で 404・カードを下げた、の 5 項目 PASS
- 吹き出しの撮影（`scripts/verify/shot-bubble.sh`、クリックなし）: 短いコマンド＝許可あり、100 字＝3 行に折り返して許可あり、150 字＝切り詰めで許可なし＋注記、固定文言「できたよ」
- 本物のボタン押下: 故障注入中に人が「許可」を押し、`pressed=allow state=allow (accepted=true)` が残った（意図した試験ではない）
