---
type: Reference
description: OpenPetsHQ/openpets（MIT・Electron）の実装解析。pet-claude が転用する設計と、転用しない設計の根拠
generated: { by: kuroko/claude-fable-5-1, at: 2026-10-21T08:20:00+09:00 }
stale_after: 2027-04-21
---

# OpenPets の実装解析（2026-10-21）

対象: OpenPetsHQ/openpets v4.1.0（MIT・Electron・pnpm monorepo）。読み取り専用で解析した。clone 先はセッションの一時領域（残さない）。参照はリポジトリ内の相対パス。

## 転用する設計（pet-claude での対応）

| OpenPets の設計 | 根拠ファイル | pet-claude での扱い |
|---|---|---|
| クリック透過＋アルファ値でヒット判定（しきい値 64・半径 1px 近傍・失敗時は「当たり」扱い） | `apps/desktop/src/pet-alpha-hit-test.ts` | Win32 の layered window（`UpdateLayeredWindow` に画素ごとのアルファ）で、アルファ 0 の画素のクリックを OS に背後へ通させる（forward 停止の監視が不要）。縁の影はアルファ 24 未満を 0 に落として拾わない（Phase 0 で実装・実測済み） |
| 最前面を定期的に取り直す（Windows は 1 秒ごと＋show/focus/blur 時）。`CalculateNativeWinOcclusion` 無効化 | `pet-window.ts` 407-432、`main.ts` 90 | 1 秒ごとの `SetWindowPos(HWND_TOPMOST)`。全画面の窓が前面の間は譲る（Phase 0 で実装・実測済み） |
| 位置はモニターごとに保存（キーは物理 bounds `"x,y,WxH"`・LRU 8 件）、workArea にクランプ、ディスプレイ変更は 200ms debounce して再クランプ | `display.ts`、`default-pet-position-state.ts`、`pet-display-coordinator.ts` | そのまま採用。ディスプレイ ID は再起動で変わるので bounds をキーにする |
| reaction（意味）→ スプライト状態（見た目）→ 吹き出し の 3 層分離。一時表示＞バッジ＞idle の優先順 | `reaction-animation-mapping.ts`、`pet-transient-policy.ts` | 「入力（hooks）は意味だけを送り、見た目はアプリが決める」を踏襲 |
| hook の作法: 非同期・timeout 3 秒・常に exit 0・固定文言のみ・プロセス共有の間引きファイル | `packages/claude/src/hooks.ts` | 同じ。間引きは同一 reaction 10 秒・吹き出し 20 秒 |
| `settings.json` は管理マーカーで自分の行だけ識別・バックアップ・一時ファイル＋rename | `packages/claude/src/hook-settings.ts` | 同じ（マーカー `--pet-claude-managed`） |
| IPC は発見ファイル `{endpoint, token, pid}`＋ランダム token | `packages/client/src/discovery.ts`、`docs/ipc.md` | loopback HTTP＋token（hook とアプリの両方から扱いやすい。Phase 2） |
| 吹き出しの入口検証: 1〜140 字・単一行・コード/URL/パス/秘密らしい文字列を拒否 | `local-ipc-protocol.ts` `validateSayMessage` | 同じ規則を `pet-core` に置く（OS 非依存なので `cargo test` で網羅。Phase 2） |
| トレイ常駐・単一インスタンス・エージェント不在でも常駐 | `main.ts` 123、`lifecycle.ts` | 同じ。Claude Code を閉じても動く要件に合う |

## 転用しない設計（理由）

| OpenPets の設計 | 理由 |
|---|---|
| Windows で `focusable: true` のまま（対策は `showInactive` だけ） | クリックでフォーカスを奪う可能性。`WS_EX_NOACTIVATE`＋`WM_MOUSEACTIVATE` に `MA_NOACTIVATE` で確実に奪わない（Phase 0 で実測） |
| renderer 経由の IPC で毎回 `setBounds` するドラッグ | Electron の制約による回り道。Win32 なら `SetCapture` して `WM_MOUSEMOVE` で動かすだけ（座標は `GetMessagePos`） |
| Windows の forward 停止を 750ms ポーリングで再武装 | マスク方式なら不要 |
| `~/.claude/CLAUDE.md` に `@import` を追記 | K の CLAUDE.md は分量規律で管理している。pet-claude は CLAUDE.md に一切書かない（hooks だけ） |
| 承認待ちは吹き出し表示のみ（`permissionDecision` 無し） | K の要件は「ペットから許可/拒否」。`PermissionRequest` を同期 hook にして `hookSpecificOutput.decision` を返す独自設計が要る |
| STT はクラウド専用（whisper-1 / ElevenLabs）、push-to-talk はトグル式、ウェイクワード無し（「Never ambient」） | K の要件はローカルで K の声のウェイクワード。whisper.cpp（whisper-rs）か faster-whisper のサイドカー、openWakeWord を別途（architecture §6） |
| プラグイン SDK・リモート操作・Linux/Wayland 分岐 | K 一人の Windows 用途には過剰 |

## OpenPets が使う hook と対応（参考値）

| hook | reaction |
|---|---|
| `UserPromptSubmit` | thinking |
| `PreToolUse` | Edit/Write/MultiEdit → editing、Bash のテスト系コマンド → testing、他は無視 |
| `PermissionRequest` | waiting＋「Approval needed」 |
| `Notification` | 無視 |
| `Stop` | success |
| `StopFailure` | error |

SessionStart・PostToolUse は未使用。

## スプライトシート（Codex 互換）

- 1 コマ 192×208、8 列。V1 は 9 行（1536×1872）、V2 は 11 行（1536×2288。行 9-10 は視線 16 方向）
- 行の意味: 0 idle(6) / 1 run-right(8) / 2 run-left(8) / 3 waving(4) / 4 jumping(5) / 5 failed(8) / 6 waiting(6) / 7 running(6) / 8 review(6)。括弧はフレーム数
- `pet.json`: `id`, `displayName`, `spritesheetPath`, 任意 `spriteVersionNumber`, `gazeAnchor`
- OpenPets に sleep / pickup の行は無い。pet-claude は V1 の 9 行を「Codex 互換」と呼び、睡眠・つままれは独自の追加行（行 9・10）で持つ。追加行が無い素材では代替行（睡眠→idle の低速再生、つままれ→waiting）にフォールバックする
