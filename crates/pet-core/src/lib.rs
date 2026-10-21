//! pet-claude の OS 非依存ロジック。
//!
//! 窓の置き場所（複数ディスプレイ・作業領域へのクランプ・位置の保存）と、
//! スプライトシートの規約（行・コマ・ポーズの代替）を持つ。Win32 には触らないので
//! どの OS でも `cargo test` で検証できる。

pub mod behavior;
pub mod compose;
pub mod geometry;
pub mod hook;
pub mod ipc;
pub mod placeholder;
pub mod placement;
pub mod sprite;
