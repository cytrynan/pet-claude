//! チャットの故障注入用の偽 `claude.exe`（`scripts/verify/chat-fault.py` が `PET_CLAUDE_EXE` で差し替える）。
//!
//! 動きは環境変数 `FAKE_MODE` で選ぶ（pet-app は子の環境から認証の変数だけを消すので、これは届く）:
//! - `ok`      : 標準入力を読み、答えの JSON（session_id 付き）を返す
//! - `error`   : `is_error: true` の JSON を終了コード 1 で返す（上限・鍵違いと同じ形）
//! - `garbage` : JSON でない標準出力と、標準エラーに 1 行を出して終了コード 2
//! - `slow`    : 孫プロセス（`FAKE_MODE=sleep`）を起こしてから 600 秒眠る（時間切れ・止める・道連れの確認）
//! - `sleep`   : 600 秒眠るだけ（孫）
//!
//! どのモードも、自分の pid を作業フォルダの `fake-pids.txt` に 1 行足す（残ったプロセスを後で調べるため）。
//! 受け取った引数と、認証の変数が見えたかを `fake-args.txt` に 1 行ずつ足す（A1・A3 の確認。鍵の値は書かない）。

use std::io::{Read, Write};
use std::time::Duration;

fn main() {
    let mode = std::env::var("FAKE_MODE").unwrap_or_else(|_| "ok".into());
    let mut pids = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("fake-pids.txt")
        .expect("作業フォルダに書けること");
    let _ = writeln!(pids, "{} {mode}", std::process::id());

    if mode != "sleep" {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let seen = |k: &str| std::env::var_os(k).is_some();
        // 送るたびに 1 行足す（続き・新しいチャットで --resume が付く・外れるのを順に見る）
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("fake-args.txt")
            .expect("作業フォルダに書けること");
        let _ = f.write_all(
            format!(
                "{} ANTHROPIC_API_KEY={} ANTHROPIC_AUTH_TOKEN={} CLAUDECODE={}\n",
                serde_json::to_string(&args).unwrap_or_default(),
                seen("ANTHROPIC_API_KEY"),
                seen("ANTHROPIC_AUTH_TOKEN"),
                seen("CLAUDECODE"),
            )
            .as_bytes(),
        );
    }

    let session = "11111111-2222-3333-4444-555555555555";
    match mode.as_str() {
        "ok" => {
            let mut input = String::new();
            let _ = std::io::stdin().read_to_string(&mut input);
            let answer = format!("{} 字を受け取ったよ。", input.chars().count());
            println!(
                "{}",
                serde_json::json!({
                    "type": "result", "subtype": "success", "is_error": false,
                    "result": answer, "session_id": session,
                })
            );
        }
        "error" => {
            println!(
                "{}",
                serde_json::json!({
                    "type": "result", "subtype": "success", "is_error": true,
                    "result": "Invalid API key · Fix external API key", "session_id": session,
                })
            );
            std::process::exit(1);
        }
        "garbage" => {
            println!("this is not json");
            eprintln!("error: something broke");
            std::process::exit(2);
        }
        "slow" => {
            let me = std::env::current_exe().expect("自分の場所");
            let _ = std::process::Command::new(me)
                .env("FAKE_MODE", "sleep")
                .spawn();
            std::thread::sleep(Duration::from_secs(600));
        }
        _ => std::thread::sleep(Duration::from_secs(600)),
    }
}
