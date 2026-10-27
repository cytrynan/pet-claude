// チャットの切替の一覧（設定画面の既定と、チャット欄の下の行のメニューで共有。文言はここだけ）。
// 値は Rust 側 pet_core::chat の Model / Effort / PermissionMode の as_str と同じ（state.json にもこの文字列で入る）。
// 並びは Claude デスクトップアプリの入力欄のメニューと同じ（番号 1〜）。K 指示 2026-10-27
const CHAT_MODELS = [
  ["opus", "Opus 5.5"],
  ["fable", "Fable 5.1"],
  ["sonnet", "Sonnet 5.5"],
  ["haiku", "Haiku 5.5"],
];
const CHAT_EFFORTS = [
  ["low", "低"],
  ["medium", "中"],
  ["high", "高"],
  ["xhigh", "最高"],
  ["max", "最大"],
];
// [値, 名前, 1 行の説明]
const CHAT_MODES = [
  ["auto", "自動", "Claude が権限の決定を処理します"],
  ["manual", "手動", "変更前に常に確認する"],
  ["acceptEdits", "編集を受け入れる", "すべてのファイル編集を自動的に承認"],
  ["plan", "プラン", "変更を加える前に計画を作成"],
  ["bypassPermissions", "権限をバイパス", "すべての権限を承認"],
];
// Rust 側と同じ既定（無い・知らない値はこれに倒す）
const CHAT_DEFAULTS = { model: "fable", effort: "high", mode: "auto" };
// 一覧から名前を引く（知らない値は既定の名前）
const chatLabel = (list, value, fallback) => {
  const hit = list.find((x) => x[0] === value) || list.find((x) => x[0] === fallback);
  return hit ? hit[1] : "";
};
