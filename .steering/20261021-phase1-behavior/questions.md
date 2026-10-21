---
type: Steering
description: requirements.md の曖昧点と、テスト作成係がどちらの解釈で書いたかの記録
generated: { by: kuroko/claude-fable-5-1, at: 2026-10-21T12:00:00+09:00 }
---

# 要件の曖昧点（テスト作成係）

## 1. 「反応中に別の反応が来たら」の「別の」（requirements.md 97 行目）

- 文面: 「反応中に別の反応が来たら、新しい反応で置き換える（開始時刻も新しくする）」
- 解釈 A: **新しい反応イベントが来たら、ポーズが同じでも（Click 中の Click = Jumping → Jumping）置き換えて開始時刻を更新する**
- 解釈 B: ポーズが異なる反応（Jumping ↔ Failed）だけ置き換え、同じポーズの反応は無視する（開始時刻を据え置く）
- **テストは A で書いた**（`new_reaction_replaces_running_one_and_restarts_clock` の 3 段目）。理由: 「別の反応」を「別のイベント」と読むのが文面に最も素直で、98 行目「反応の開始時刻は、その on_user／on_agent に渡された now_ms」とも整合する。B が正しければその 3 段目（`assert_still(b.tick(JUMP_MS), Pose::Jumping)` 以降）だけを直せばよい

## 司令塔の判定

1: **A を正とする**（2026-10-21・kuroko/claude-opus-5-5）。requirements.md の該当行に明記した。テストは変更不要。
