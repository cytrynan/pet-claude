//! 固定文言の島の大きさを出す（見た目の検証 `scripts/verify/render-ui.js` が、描く窓の大きさを
//! アプリと同じ関数から取るための道具）。
//! 使い方: cargo run -q -p pet-core --example bubble_size -- '["文1","文2"]'
//! 出力: [{"w":320,"h":60,"lines":2}, ...]（論理ピクセル。影の余白は含まない）

use pet_core::bubble::{STATUS_MAX_WIDTH, STATUS_PAD_X, status_island, wrap};

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_else(|| "[]".to_string());
    let texts: Vec<String> = serde_json::from_str(&arg).expect("JSON の文字列の配列");
    let out: Vec<serde_json::Value> = texts
        .iter()
        .map(|t| {
            let (w, h) = status_island(t);
            let lines = wrap(t, STATUS_MAX_WIDTH - STATUS_PAD_X * 2.0).lines;
            serde_json::json!({ "w": w, "h": h, "lines": lines })
        })
        .collect();
    println!("{}", serde_json::Value::Array(out));
}
