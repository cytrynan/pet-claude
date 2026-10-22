//! 永続化する状態（`%LOCALAPPDATA%\pet-claude\state.json`）。
//! 書き込みは一時ファイル＋rename（途中で落ちても壊れたファイルを残さない）。

use std::fs;

use pet_core::chat::AuthMode;
use pet_core::placement::PositionStore;
use serde_json::{Value, json};

use crate::log::{data_dir, log};

pub const DEFAULT_SCALE: f32 = 0.55;
pub const MIN_SCALE: f32 = 0.25;
pub const MAX_SCALE: f32 = 1.5;

#[derive(Debug, Clone)]
pub struct AppState {
    /// 1 コマ（192×208）に掛ける倍率。表示倍率 100% での大きさ。
    pub scale: f32,
    pub positions: PositionStore,
    /// 声で知らせる（Phase 3 で使う。いまは保存だけ）
    pub tts: bool,
    /// 呼びかけで音声入力を始める（Phase 4 で使う。いまは保存だけ）
    pub wake: bool,
    /// 放置すると散歩する（K 指示 2026-10-21: 既定はオフ。睡眠は対象外）
    pub walk: bool,
    /// 表示するペットの id（`pets::discover` の id。None は内蔵の仮キャラ）
    pub pet: Option<String>,
    /// チャットの接続（K 決定 2026-10-21: 既定はサブスク）。API キーそのものは資格情報マネージャー（cred.rs）
    pub chat_auth: AuthMode,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            scale: DEFAULT_SCALE,
            positions: PositionStore::default(),
            tts: true,
            wake: false,
            walk: false,
            pet: None,
            chat_auth: AuthMode::Subscription,
        }
    }
}

impl AppState {
    pub fn load() -> Self {
        let path = data_dir().join("state.json");
        let Ok(text) = fs::read_to_string(&path) else {
            return Self::default();
        };
        let Ok(v) = serde_json::from_str::<Value>(&text) else {
            log(format!(
                "state.json を読めないので既定値で起動: {}",
                path.display()
            ));
            return Self::default();
        };
        let d = Self::default();
        Self {
            scale: v["scale"]
                .as_f64()
                .map_or(d.scale, |s| (s as f32).clamp(MIN_SCALE, MAX_SCALE)),
            positions: serde_json::from_value(v["positions"].clone()).unwrap_or_default(),
            tts: v["tts"].as_bool().unwrap_or(d.tts),
            wake: v["wake"].as_bool().unwrap_or(d.wake),
            walk: v["walk"].as_bool().unwrap_or(d.walk),
            pet: v["pet"].as_str().map(str::to_string),
            // 無い・知らない値はサブスク（from_state の規則）
            chat_auth: AuthMode::from_state(v["chat_auth"].as_str()),
        }
    }

    pub fn save(&self) {
        let dir = data_dir();
        let _ = fs::create_dir_all(&dir);
        let body = json!({
            "version": 1,
            "scale": self.scale,
            "positions": self.positions,
            "tts": self.tts,
            "wake": self.wake,
            "walk": self.walk,
            "pet": self.pet,
            "chat_auth": self.chat_auth.as_str(),
        });
        let tmp = dir.join("state.json.tmp");
        let ok = fs::write(&tmp, serde_json::to_vec_pretty(&body).unwrap_or_default())
            .and_then(|()| fs::rename(&tmp, dir.join("state.json")));
        if let Err(e) = ok {
            log(format!("state.json の保存に失敗: {e}"));
        }
    }
}
