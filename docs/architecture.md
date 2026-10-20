---
type: Architecture
description: pet-claude の技術仕様。Python 3.12 + PySide6 の単一プロセス。入力（hooks）・行動・描画・声を分離する
generated: { by: kuroko/claude-fable-5-1, at: 2026-10-21T08:40:00+09:00 }
---

# pet-claude 技術仕様書

## 1. 技術選定

| 候補 | 判定 | 理由 |
|---|---|---|
| **Python 3.12 + PySide6（Qt 6）** | **採用** | 透過・最前面・フォーカス非取得・マスクによる不透明部分だけのクリック判定・ディスプレイ/倍率の扱いが Qt の標準機能で揃う。声（AivisSpeech HTTP・faster-whisper・openWakeWord）が同じ言語で書ける。K の環境に Python と uv がある |
| Electron（OpenPets 方式） | 不採用 | クリック透過は forward 方式で Windows では止まることがあり監視が要る。ローカル STT・ウェイクワードを別プロセスにせざるを得ない |
| Tauri 2（zzp1221 方式） | 不採用 | K の環境に Rust ツールチェーンが無い。声のためにいずれ Python が要る |

## 2. 全体構成（単一プロセス＋hook プロセス）

```
Claude Code ──hook(stdin JSON)──▶ hooks/pet_hook.py ──HTTP(loopback+token)──▶ ┐
                                     ▲ PermissionRequest だけ同期で待つ            │
                                     └────────── decision ◀──────────────────── │
                                                                                 ▼
                                   pet-claude（PySide6・トレイ常駐・単一インスタンス）
                                   ┌───────────┐  ┌──────────┐  ┌──────────┐  ┌────────┐
                                   │ inputs/   │─▶│ behavior/│─▶│ render/  │  │ voice/ │
                                   │ hooks受信 │  │ 状態機械 │  │ 窓・絵   │  │ TTS/STT│
                                   │ manual    │  │ 優先順位 │  │ 吹き出し │  │ wake   │
                                   └───────────┘  └──────────┘  └──────────┘  └────────┘
                                                        ▲  settings/（JSON）・assets/pets/<name>/
```

- **inputs/**: 外から来る出来事を `AgentEvent` に正規化する。`HooksInput`（HTTP 受信）と `ManualInput`（CLI・テスト用）。差し替え可能（F12）
- **behavior/**: `AgentEvent` と時間・マウス操作から「いまのポーズ」を決める状態機械。Qt に依存しない純 Python（pytest で網羅）
- **render/**: Qt の窓・スプライト描画・吹き出し・バー・設定画面・トレイ
- **voice/**: AivisSpeech への HTTP、faster-whisper、openWakeWord。すべて任意で、無くても動く
- **hooks/**: Claude Code から呼ばれる小さなスクリプト。アプリと別プロセス

## 3. 窓（render/window.py）

| 要件 | 実装 |
|---|---|
| 透過・枠なし | `Qt.FramelessWindowHint` + `WA_TranslucentBackground` |
| 最前面・タスクバー非表示 | `Qt.WindowStaysOnTopHint` + `Qt.Tool` |
| フォーカスを奪わない（F2） | `Qt.WindowDoesNotAcceptFocus`（Windows では `WS_EX_NOACTIVATE`）。足りなければ `nativeEvent` で `WM_MOUSEACTIVATE` に `MA_NOACTIVATE` を返す |
| 不透明部分だけクリック（F3） | 現在のフレーム画像のアルファ（しきい値 64）から `QRegion` を作り `setMask()`。フレームが変わるたびに更新（192×208 なら十分軽い）。透明部分のクリックは OS が背後に通す |
| ドラッグ（F4） | `mousePressEvent` で掴み、`mouseMoveEvent` で `move()`。離したら `clamp()` |
| クランプ（F4） | 窓の中心に最も近い `QScreen.availableGeometry()`（作業領域）に収める。位置はディスプレイの `geometry` を鍵（`"x,y,WxH"`）に保存（LRU 8 件） |
| ディスプレイ変化（F5） | `QGuiApplication.screenAdded/screenRemoved` と各 `QScreen.geometryChanged` を 200ms debounce して再クランプ |
| 最前面の取り直し（F6） | 1 秒ごとに `SetWindowPos(hwnd, HWND_TOPMOST, …, SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE)`（ctypes）。Phase 0 で必要性を実測し、不要なら外す |
| 倍率 | Qt の論理座標で扱う。`devicePixelRatio` は描画だけが見る |

## 4. 行動（behavior/）

### 4.1 入力の型

```python
@dataclass(frozen=True)
class AgentEvent:
    kind: Literal["prompt", "tool_start", "needs_input", "permission", "done", "error", "idle"]
    session_id: str
    detail: str = ""          # 吹き出し向けの短文（検証済み）
    permission_id: str = ""   # kind=="permission" のとき。返事の宛先
```

### 4.2 ポーズと優先順位（F8）

```
Grabbed（ドラッグ中）
 > Reaction（クリック: joy / 完了: celebrate を 1 回だけ）
 > Agent: needs_input(wave) > error(failed) > working(running)
 > Autonomous: sleep > walk > idle
```

- 自律行動は `idle` を基本に、無操作 90 秒〜で `walk`（作業領域の端で折り返す）、5 分〜で `sleep`。Agent 状態が入ったら自律行動は中断
- Agent 状態は最後のイベントから 120 秒で `idle` に戻す（hook が落ちても固まらない）
- 複数セッションが同時に動くときはセッションごとに状態を持ち、`needs_input > error > working` の順で代表を選ぶ

### 4.3 スプライトの行（素材規約・F9）

| 行 | ポーズ | 代替（行が無いとき） |
|---|---|---|
| 0 idle / 1 run-right / 2 run-left / 3 waving / 4 jumping / 5 failed / 6 waiting / 7 running / 8 review | Codex 互換 | — |
| 9 sleep | 独自 | idle を半速 |
| 10 grabbed | 独自 | waiting |

`assets/pets/<name>/pet.json` の `rows` で行番号を上書きできる。描画は `QTimer` でフレーム番号を進め、`QPixmap.copy()` でコマを切り出す。

## 5. Claude Code 連携（hooks/・inputs/hooks_input.py）

### 5.1 登録する hook

| hook | 送る `AgentEvent` | 同期 |
|---|---|---|
| `UserPromptSubmit` | prompt | 非同期（`async: true`, timeout 3） |
| `PreToolUse`（`*`） | tool_start（ツール名のみ） | 非同期 |
| `PermissionRequest`（`*`） | permission（ツール名＋要約） | **同期**（timeout 60）。返事が来るまで待つ |
| `Notification` | needs_input（承認以外の入力待ち） | 非同期 |
| `Stop` | done | 非同期 |
| `StopFailure` | error | 非同期 |

`~/.claude/settings.json` へは `--pet-claude-managed` マーカー付きで追記し、追記前にバックアップを作る。`CLAUDE.md` には書かない（F15）。

### 5.2 承認の往復（F13）

1. hook が `POST /permission` でアプリに登録し、`GET /permission/<id>` を 500ms ごとに最大 60 秒ポーリングする
2. アプリは吹き出しに「許可」「拒否」を出す。押されたら返事を保存
3. hook は返事を `hookSpecificOutput: {hookEventName: "PermissionRequest", decision: {behavior: "allow"|"deny"}}` で stdout に返す
4. 60 秒以内に返事が無ければ `{}` を返して通常の端末プロンプトに戻す。アプリが居なければ即 `{}`

`decision` の正確なスキーマは実装時に公式ドキュメントで再確認する（この表は 2026-10 時点の理解）。

### 5.3 IPC

- アプリ起動時に `127.0.0.1` の空きポートで HTTP を聞き、`%LOCALAPPDATA%\pet-claude\runtime\ipc.json` に `{endpoint, token, pid}` を書く
- hook は token を `Authorization` に付ける。接続 300ms・応答 500ms で諦めて exit 0
- 間引きは `%LOCALAPPDATA%\pet-claude\runtime\throttle.json`（同じ kind は 10 秒）

## 6. 声（voice/）

| 部品 | 実装 | 無いとき |
|---|---|---|
| TTS | AivisSpeech `POST /audio_query` → `POST /synthesis`（`127.0.0.1:10101`、話者「天深シノ」ノーマル。ai-news の `dev\ai-news-routine\lib\radio.mjs` と同じ）→ `QSoundEffect` で再生 | 起動していなければ黙って無効 |
| STT | faster-whisper（`small`・日本語・CPU int8）。`sounddevice` で最大 30 秒録音 | モデル未取得なら設定画面で案内 |
| ウェイクワード | openWakeWord。K の録音から独自モデルを学習（Phase 4 で調査） | 既定 OFF |
| チャット | Claude Agent SDK（Python）で新規セッション。答えは吹き出し＋TTS | U1 の決定待ち |

話す文言は固定文言プール（「できたよ」「許可待ってる」等）＋検証済みの短文のみ（N3）。

## 7. 設定と保存先

- `%LOCALAPPDATA%\pet-claude\settings.json`: `scale`（0.5〜1.5）、`tts_enabled`、`wake_enabled`、`pet`（素材名）、`positions`
- 書き込みは一時ファイル＋rename
- 自動起動は Windows タスク（`run-hidden.vbs` 経由・N5）

## 8. 検証

- 純 Python（behavior・clamp・hook の解析・サニタイズ）は pytest で 80% 以上
- 窓まわりは Phase 0 のチェックリスト（`docs/checklists/phase0-window.md`）を 2 画面×倍率違いで実測し、スクショを `.steering/` に残す
- hook は故障注入 3 件（アプリ停止／token 不一致／応答遅延）で「200ms 以内に exit 0」を確認するテストを `tests/` に置く
- 差分ファジング: behavior の状態機械は変更のたびに旧実装と乱数イベント列で突き合わせる
