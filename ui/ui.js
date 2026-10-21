// pet-claude の UI 共通。Tauri の invoke（withGlobalTauri）を包む
const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);

// Esc で閉じる（フォーカスを持つ窓だけに効く）
window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") invoke("ui_action", { action: "close" });
});
