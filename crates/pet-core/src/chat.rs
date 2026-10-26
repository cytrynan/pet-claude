//! チャット（`claude -p` の子プロセス）まわりの純粋な関数。
//!
//! 子プロセスに渡す引数・環境変数の組み立て、答えの JSON の解析、吹き出しとチャット欄に出す文の
//! 整形、実行ファイルの探索を持つ。OS にもファイルにもネットワークにも触らない（ファイルの有無は
//! 引数の関数で受け取る）。答えもエラーの文も外（モデル・CLI）から来る信頼できないデータなので、
//! どの経路でも panic しない。
//! 仕様の正本は `.steering/20261021-phase3-chat/core-requirements.md`。

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::hook::sanitize_line;

/// 子プロセスに使うモデル。
pub const MODEL: &str = "sonnet";

/// 子プロセスを待つ最大の秒数。
pub const TIMEOUT_SECS: u64 = 120;

/// 吹き出しに出す文の最大文字数（Unicode のスカラー値で数える）。
pub const BUBBLE_MAX_CHARS: usize = 140;

/// チャット欄に出す文の最大文字数。
pub const PANEL_MAX_CHARS: usize = 4000;

/// API キーを入れておく資格情報ストアの名前。
pub const CREDENTIAL_TARGET: &str = "pet-claude/anthropic-api-key";

/// 送る本文の最大文字数（前後の空白を落とした後に数える）。
pub const MAX_INPUT_CHARS: usize = 2000;

/// 失敗の理由（1 行）の最大文字数。吹き出しより長く取り、パスなどもチャット欄では見えるようにする。
pub const FAILURE_MAX_CHARS: usize = 200;

/// 吹き出しの文が空だったときに出す文。
const EMPTY_TEXT: &str = "（答えが空でした）";

/// 吹き出しに出せない文の代わりに出す文（中身はチャット欄で見られる）。
pub const BUBBLE_HIDDEN: &str = "（答えはチャット欄に出しました）";

/// 秘密らしい文字列を伏せるときの置き換え。
const MASK: &str = "••••";

/// 秘密らしい並びと見なす、英数字・`_`・`-` の連続の最小の長さ。
/// API キーやセッション id（36 字）は 32 字を超えるが、普通の単語はこれに届かない。
const SECRET_RUN_MIN: usize = 32;

/// 子プロセスの認証の方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    /// Claude のサブスク（既定）。
    Subscription,
    /// Anthropic の API キー（従量課金）。
    ApiKey,
}

impl AuthMode {
    /// 保存した状態の文字列から戻す。`"api_key"` だけが `ApiKey`。
    ///
    /// 知らない値や壊れた値で API 課金に倒れないよう、それ以外はすべてサブスクにする。
    pub fn from_state(v: Option<&str>) -> AuthMode {
        match v {
            Some("api_key") => AuthMode::ApiKey,
            _ => AuthMode::Subscription,
        }
    }

    /// 状態に保存する文字列。`from_state` と往復できる。
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthMode::Subscription => "subscription",
            AuthMode::ApiKey => "api_key",
        }
    }
}

/// 子プロセスの準備の失敗。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatError {
    /// API キーの方式なのに、キーが無い（空・空白だけを含む）。
    MissingKey,
}

/// 子プロセスの結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatOutcome {
    /// 答えが取れた。`text` は生の文字列（表示の時に `bubble_text`・`panel_text` を通す）。
    Answer {
        text: String,
        session_id: Option<String>,
    },
    /// 失敗。`reason` は吹き出しにそのまま出せる 1 行。
    Failed {
        reason: String,
        session_id: Option<String>,
    },
}

/// Claude Code のセッション id（UUID の文字列）か。
///
/// `--resume` の引数に外から来た値を混ぜないため、`8-4-4-4-12` 桁の 16 進を
/// ちょうど 36 字で受ける形に絞る。
pub fn is_session_id(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, &c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

/// 子プロセス（`claude`）に渡す引数。本文（ユーザーの文）は標準入力で渡すので含めない。
///
/// 組み込みツール・MCP・フック・スラッシュコマンドをすべて外し、1 ターンで答えさせる。
/// `resume` は `is_session_id` を満たす時だけ末尾に付ける（満たさなければ新しい会話にする）。
pub fn child_args(system_prompt_file: &str, resume: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--output-format",
        "json",
        "--safe-mode",
        "--model",
        MODEL,
        // 空文字 1 つで組み込みツールを全部外す
        "--tools",
        "",
        "--disallowedTools",
        "mcp__*",
        "--strict-mcp-config",
        "--settings",
        r#"{"disableAllHooks":true}"#,
        "--disable-slash-commands",
        "--permission-prompts",
        "none",
        "--max-turns",
        "1",
        "--system-prompt-file",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    args.push(system_prompt_file.to_string());
    if let Some(id) = resume.filter(|id| is_session_id(id)) {
        args.push("--resume".to_string());
        args.push(id.to_string());
    }
    args
}

/// 子プロセスに渡さない環境変数の名前。
///
/// 認証や接続先を切り替える変数と、入れ子の `claude` だと判定させる変数。環境に
/// `ANTHROPIC_API_KEY` があると `claude -p` は必ずそれを使うので、サブスクのつもりで
/// 知らないうちに API 課金になるのを防ぐ。
const STRIPPED_ENV: [&str; 8] = [
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
    "ANTHROPIC_PROFILE",
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
];

/// 子プロセスに渡す環境変数の並び。
///
/// 親の環境から `STRIPPED_ENV` を取り除く。Windows の環境変数名は大文字小文字を区別しないので、
/// ASCII の大文字小文字を無視して一致させる。`ApiKey` のときだけ、前後の空白を落とした
/// キーを `ANTHROPIC_API_KEY` として末尾に 1 つ足す（キーが空なら `MissingKey`）。
pub fn child_env(
    mode: AuthMode,
    parent: &[(String, String)],
    api_key: Option<&str>,
) -> Result<Vec<(String, String)>, ChatError> {
    let mut env: Vec<(String, String)> = parent
        .iter()
        .filter(|(name, _)| !STRIPPED_ENV.iter().any(|s| name.eq_ignore_ascii_case(s)))
        .cloned()
        .collect();
    if mode == AuthMode::ApiKey {
        let key = api_key.map(str::trim).unwrap_or("");
        if key.is_empty() {
            return Err(ChatError::MissingKey);
        }
        env.push(("ANTHROPIC_API_KEY".to_string(), key.to_string()));
    }
    Ok(env)
}

/// `claude -p --output-format json` の結果を読む。
///
/// 失敗の理由に使う 1 行。秘密らしい文字列を伏せ、1 行にして 200 字に収める。空なら `default`。
///
/// 吹き出しと違って URL やパスは隠さない（吹き出しに出す時はアプリ側が `bubble_text` を通す）。
fn failure_text(s: &str, default: &str) -> String {
    let line = sanitize_line(&mask_secrets(s), FAILURE_MAX_CHARS);
    if line.is_empty() {
        default.to_string()
    } else {
        line
    }
}

/// 標準出力が `type: "result"` の JSON オブジェクトなら、終了コードに関わらずそれに従う
/// （上限・鍵違いは終了コード 1 で `is_error: true` の JSON を出すため）。読めなければ
/// 標準エラーの最初の行を理由にする。
pub fn parse_output(exit_code: Option<i32>, stdout: &[u8], stderr: &[u8]) -> ChatOutcome {
    if let Some(obj) = result_object(stdout) {
        let session_id = obj
            .get("session_id")
            .and_then(Value::as_str)
            .filter(|s| is_session_id(s))
            .map(str::to_string);
        let result = obj
            .get("result")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        let is_error = obj.get("is_error").and_then(Value::as_bool) == Some(true);
        return if is_error {
            ChatOutcome::Failed {
                reason: failure_text(result.unwrap_or(""), "Claude Code がエラーを返しました"),
                session_id,
            }
        } else if let Some(text) = result {
            ChatOutcome::Answer {
                text: text.to_string(),
                session_id,
            }
        } else {
            ChatOutcome::Failed {
                reason: "答えが空でした".to_string(),
                session_id,
            }
        };
    }

    let stderr = String::from_utf8_lossy(stderr);
    let code = exit_code.map_or_else(|| "なし".to_string(), |c| c.to_string());
    let default = format!("Claude Code が答えませんでした（終了コード {code}）");
    let reason = stderr
        .lines()
        .find(|l| !l.trim().is_empty())
        .map_or_else(|| default.clone(), |l| failure_text(l, &default));
    ChatOutcome::Failed {
        reason,
        session_id: None,
    }
}

/// 標準出力が `type: "result"` の JSON オブジェクト 1 つならそのオブジェクトを返す。
/// UTF-8 でない・JSON でない・後ろに余分な文字がある・オブジェクトでない・`type` が違う、は `None`。
fn result_object(stdout: &[u8]) -> Option<serde_json::Map<String, Value>> {
    let text = std::str::from_utf8(stdout).ok()?;
    match serde_json::from_str::<Value>(text.trim()).ok()? {
        Value::Object(obj) if obj.get("type").and_then(Value::as_str) == Some("result") => {
            Some(obj)
        }
        _ => None,
    }
}

/// 吹き出しに出す 1 行（1〜140 字・単一行）。
///
/// URL・パス・コード・秘密らしい文字列を含む文は出さず、チャット欄へ誘導する文を返す。
/// 検査は切り詰める前の 1 行で行う。切り詰めた後で検査すると、境界をまたぐ鍵の先頭
/// （`sk-` など）が検査をすり抜けて吹き出しに出てしまうため。
pub fn bubble_text(s: &str) -> String {
    let line = sanitize_line(s, usize::MAX);
    if line.is_empty() {
        return EMPTY_TEXT.to_string();
    }
    if is_unsafe_for_bubble(&line) {
        return BUBBLE_HIDDEN.to_string();
    }
    sanitize_line(&line, BUBBLE_MAX_CHARS)
}

/// 吹き出しに出してはいけない文か（URL・パス・コード・秘密らしい文字列を含む）。
/// 質問のカード（`crate::ask`）も同じ判定を使う（集合を 1 か所に寄せる）。
pub fn is_unsafe_for_bubble(line: &str) -> bool {
    // sk-・www.・:// は大文字小文字を無視する。ASCII だけを小文字にするので文字の位置は変わらない
    let lower = line.to_ascii_lowercase();
    if lower.contains("://") || lower.contains("www.") {
        return true;
    }
    if line.contains('`') {
        return true;
    }
    has_path(line) || !secret_spans(line).is_empty()
}

/// パスらしい並び（ドライブ・UNC・ホームや典型的なディレクトリ）を含むか。
fn has_path(line: &str) -> bool {
    if line.contains("\\\\")
        || line.contains("~/")
        || line.contains("/Users/")
        || line.contains("/home/")
        || line.contains("/etc/")
    {
        return true;
    }
    let chars: Vec<char> = line.chars().collect();
    chars
        .windows(3)
        .any(|w| w[0].is_ascii_alphabetic() && w[1] == ':' && (w[2] == '\\' || w[2] == '/'))
}

/// 秘密らしい部分のバイト範囲（小さい順・重ならない）。`bubble_text` の判定と `mask_secrets` の
/// 置き換えが同じ結果になるよう、判定をここ 1 か所に寄せる。
///
/// 英数字・`_`・`-` の連続（ASCII のみ。日本語は含まない）を 1 つの単位として見て、
/// - `SECRET_RUN_MIN` 字以上なら全体
/// - そうでなければ、直前が ASCII 英数字でない `sk-`（大文字小文字を無視）から単位の終わりまで
///   （`task-` のような語の途中は対象外。直前が `_`・`-` なら対象）
fn secret_spans(s: &str) -> Vec<(usize, usize)> {
    let is_token = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    let mut spans = Vec::new();
    let mut chars = s.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        if !is_token(c) {
            continue;
        }
        let mut end = start + c.len_utf8();
        let mut count = 1usize;
        while let Some(&(i, n)) = chars.peek() {
            if !is_token(n) {
                break;
            }
            end = i + n.len_utf8();
            count += 1;
            chars.next();
        }
        // 単位は ASCII だけなのでバイト位置と文字位置が一致する
        let run = &s[start..end];
        if count >= SECRET_RUN_MIN {
            spans.push((start, end));
            continue;
        }
        let lower = run.to_ascii_lowercase();
        let hit = lower.match_indices("sk-").map(|(i, _)| i).find(|&i| {
            i == 0
                || run[..i]
                    .chars()
                    .next_back()
                    .is_some_and(|p| !p.is_ascii_alphanumeric())
        });
        if let Some(i) = hit {
            spans.push((start + i, end));
        }
    }
    spans
}

/// 秘密らしい文字列（`sk-` で始まる語、32 字以上の英数字の並び）を `••••` に置き換える。
///
/// それ以外の文字はそのまま。秘密が無ければ入力と同じ文字列を返す。
pub fn mask_secrets(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    for (start, end) in secret_spans(s) {
        out.push_str(&s[last..start]);
        out.push_str(MASK);
        last = end;
    }
    out.push_str(&s[last..]);
    out
}

/// 子プロセスに送る本文。前後の空白を落とし、空か `MAX_INPUT_CHARS` 字超なら `None`。
///
/// 長すぎる本文は切り詰めず断る（途中で切れた文を送ると意味が変わるため）。
pub fn input_text(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty() && t.chars().count() <= MAX_INPUT_CHARS).then(|| t.to_string())
}

/// チャット欄に出す全文。改行は残し、4000 字を超えたら切り詰める。
///
/// 吹き出しと違って URL・パス・コードは消さない（吹き出しから誘導される先なので）。秘密らしい
/// 文字列だけは伏せる。
pub fn panel_text(s: &str) -> String {
    // 最初に秘密を伏せる（チャット欄にも秘密らしい文字列は出さない）
    let masked = mask_secrets(s);
    let unified = masked.replace("\r\n", "\n").replace('\r', "\n");
    let cleaned: String = unified
        .chars()
        .map(|c| if c != '\n' && c.is_control() { ' ' } else { c })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return EMPTY_TEXT.to_string();
    }
    if trimmed.chars().count() <= PANEL_MAX_CHARS {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(PANEL_MAX_CHARS - 1).collect();
    out.push('…');
    out
}

/// `claude` の実行ファイルを探す。
///
/// PATH のフォルダを先頭から見て `claude.exe`、無ければ npm 版（`claude.cmd` が隣にあり、
/// `node_modules` の中に本体の exe がある）の exe を返す。どちらも無ければ `home` のネイティブ版。
/// `.cmd` 自体は返さない（Windows で `.cmd` を直接起動すると引数の解釈が変わるため）。
/// 空のパスのフォルダは飛ばす（カレントフォルダの実行ファイルを拾わない）。
pub fn find_claude(
    path_dirs: &[PathBuf],
    home: Option<&Path>,
    exists: &dyn Fn(&Path) -> bool,
) -> Option<PathBuf> {
    for d in path_dirs.iter().filter(|d| !d.as_os_str().is_empty()) {
        let exe = d.join("claude.exe");
        if exists(&exe) {
            return Some(exe);
        }
        let npm_exe = d
            .join("node_modules")
            .join("@anthropic-ai")
            .join("claude-code")
            .join("bin")
            .join("claude.exe");
        if exists(&d.join("claude.cmd")) && exists(&npm_exe) {
            return Some(npm_exe);
        }
    }
    let native = home?.join(".local").join("bin").join("claude.exe");
    exists(&native).then_some(native)
}

/// `claude auth status` の標準出力から、設定画面に出す短い文を作る。
///
/// メールアドレス・組織名は出さない。`subscriptionType` は英数字だけの時だけ使う
/// （外から来た文字列をそのまま画面に出さないため）。
pub fn auth_status_label(stdout: &[u8]) -> String {
    const UNKNOWN: &str = "確かめられませんでした";
    let Ok(Value::Object(obj)) = serde_json::from_slice::<Value>(stdout) else {
        return UNKNOWN.to_string();
    };
    match obj.get("loggedIn").and_then(Value::as_bool) {
        None => UNKNOWN.to_string(),
        Some(false) => "ログインしていません".to_string(),
        Some(true) => match obj.get("authMethod").and_then(Value::as_str) {
            Some("claude.ai") => {
                match obj
                    .get("subscriptionType")
                    .and_then(Value::as_str)
                    .filter(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_alphanumeric()))
                {
                    Some(t) => format!("サブスク（{t}）"),
                    None => "サブスク".to_string(),
                }
            }
            Some("api_key") => "API キー".to_string(),
            _ => "その他の接続".to_string(),
        },
    }
}
