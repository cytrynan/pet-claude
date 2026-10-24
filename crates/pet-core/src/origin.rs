//! 承認の要求がどこから来たか（ターミナルの Claude Code か、Claude デスクトップアプリか）の判定と、
//! 承認のカードの「〜で」の文言。
//!
//! 目的: カードの「ターミナルで」（答えを Claude Code 本体の画面に戻すボタン）は、デスクトップアプリから
//! 来た要求では誤りになる（K は CLI とアプリの両方を使う）。来た先に合わせて文言を変える。
//!
//! 手がかりは hook に渡る環境変数（2026-10-24 実測。`.steering/20261024-permission-origin/brief.md` に記録）:
//! - `CLAUDE_CODE_ENTRYPOINT`: 起動経路。ターミナルの対話は `cli`、`-p` は `sdk-cli`、アプリは
//!   `claude-desktop`。値の一覧は公式の監視ドキュメントの `app.entrypoint` にあるが、**環境変数として
//!   渡ることは公式に書かれていない**（実測だけが根拠）
//! - ただし Claude Code はこの値が既にあれば上書きしない。アプリのセッションの中（Bash ツール・アプリの
//!   ターミナル）から起こした `claude` は `claude-desktop` を引き継ぐ。そこで「アプリが起こした
//!   Claude Code そのものか」を、アプリが付ける `CLAUDE_CODE_EXECPATH` と、hook の親の Claude Code
//!   （公式に記載のある `CLAUDE_PID`）の実行ファイルが同じかで確かめる。入れ子なら別の実行ファイルになる
//!
//! 判定できないとき（入れ子・`-p`・VS Code・古い pet-hook）は中立の「Claude Code で」にする。
//! どこで答えるかを言い当てられない時に、外れた場所を指すより、どこでも正しい言い方を選ぶ。

/// 承認の要求の来た先。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// ターミナルで動く対話の Claude Code（`CLAUDE_CODE_ENTRYPOINT=cli`）。
    Terminal,
    /// Claude デスクトップアプリ（Code タブ）が起こした Claude Code。
    DesktopApp,
    /// 分からない。
    Unknown,
}

/// デスクトップアプリが自分で起こしたセッションの起動経路（公式の監視ドキュメントの記載）。
const DESKTOP_ENTRYPOINTS: [&str; 3] = ["claude-desktop", "claude-desktop-3p", "local-agent"];

/// 来た先を決める。
///
/// - `entrypoint`: `CLAUDE_CODE_ENTRYPOINT` の値
/// - `exec_path`: `CLAUDE_CODE_EXECPATH` の値（アプリが起こした時だけ付く）
/// - `claude_image`: `CLAUDE_PID` のプロセスの実行ファイルのフルパス（取れなければ None）
pub fn classify(
    entrypoint: Option<&str>,
    exec_path: Option<&str>,
    claude_image: Option<&str>,
) -> Origin {
    match entrypoint {
        Some("cli") => Origin::Terminal,
        Some(ep) if DESKTOP_ENTRYPOINTS.contains(&ep) => {
            // 入れ子（アプリのセッションの中から起こした claude）を弾く。両方そろって同じ時だけアプリ
            match (exec_path, claude_image) {
                (Some(a), Some(b)) if !a.is_empty() && same_path(a, b) => Origin::DesktopApp,
                _ => Origin::Unknown,
            }
        }
        _ => Origin::Unknown,
    }
}

/// Windows のパスとして同じか（大文字小文字と区切りの向きを無視）。
fn same_path(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace('/', "\\").to_lowercase();
    norm(a) == norm(b)
}

impl Origin {
    /// hook からアプリへ送るときの名前（`WireEvent::origin`）。
    pub fn as_str(&self) -> &'static str {
        match self {
            Origin::Terminal => "terminal",
            Origin::DesktopApp => "desktop_app",
            Origin::Unknown => "unknown",
        }
    }

    /// `WireEvent::origin` から戻す。無い（古い pet-hook）・知らない値は `Unknown`。
    pub fn from_wire(s: Option<&str>) -> Origin {
        match s {
            Some("terminal") => Origin::Terminal,
            Some("desktop_app") => Origin::DesktopApp,
            _ => Origin::Unknown,
        }
    }

    /// カードの「決めない」ボタンの文言。
    pub fn ask_label(&self) -> &'static str {
        match self {
            Origin::Terminal => "ターミナルで",
            Origin::DesktopApp => "Claude アプリで",
            Origin::Unknown => "Claude Code で",
        }
    }

    /// 「許可」を出せないときの注記。
    pub fn note(&self) -> &'static str {
        match self {
            Origin::Terminal => "全文が見えないので、許可はターミナルで",
            Origin::DesktopApp => "全文が見えないので、許可は Claude アプリで",
            Origin::Unknown => "全文が見えないので、許可は Claude Code で",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUNDLED: &str =
        r"C:\Users\u\AppData\Roaming\Claude\claude-code\2.1.308\c17a819afb40\claude.exe";

    #[test]
    fn terminal_cli_is_terminal() {
        // 実測: きれいな環境のターミナルで起こした対話の claude
        assert_eq!(classify(Some("cli"), None, None), Origin::Terminal);
        assert_eq!(
            classify(Some("cli"), None, Some(r"C:\x\claude.exe")),
            Origin::Terminal
        );
    }

    #[test]
    fn desktop_hosted_session_is_app() {
        // 実測: アプリの Code タブのセッション（EXECPATH と CLAUDE_PID の実行ファイルが同じ）
        assert_eq!(
            classify(Some("claude-desktop"), Some(BUNDLED), Some(BUNDLED)),
            Origin::DesktopApp
        );
        // 大文字小文字・区切りの向きの違いは同じ場所とみなす（Windows のパス）
        let other_form = BUNDLED.to_uppercase().replace('\\', "/");
        assert_eq!(
            classify(Some("claude-desktop"), Some(BUNDLED), Some(&other_form)),
            Origin::DesktopApp
        );
        for ep in ["claude-desktop-3p", "local-agent"] {
            assert_eq!(
                classify(Some(ep), Some(BUNDLED), Some(BUNDLED)),
                Origin::DesktopApp
            );
        }
    }

    #[test]
    fn nested_claude_inside_desktop_env_is_unknown() {
        // 実測: アプリのセッションの Bash から起こした claude は claude-desktop と EXECPATH を引き継ぐが、
        // 自分の実行ファイルは npm の claude になる
        let npm =
            r"C:\Users\u\AppData\Roaming\npm\node_modules\@anthropic-ai\claude-code\bin\claude.exe";
        assert_eq!(
            classify(Some("claude-desktop"), Some(BUNDLED), Some(npm)),
            Origin::Unknown
        );
        // 確かめる材料が欠けたら決めない
        assert_eq!(
            classify(Some("claude-desktop"), None, Some(BUNDLED)),
            Origin::Unknown
        );
        assert_eq!(
            classify(Some("claude-desktop"), Some(BUNDLED), None),
            Origin::Unknown
        );
        assert_eq!(
            classify(Some("claude-desktop"), Some(""), Some("")),
            Origin::Unknown
        );
    }

    #[test]
    fn other_entrypoints_are_unknown() {
        for ep in [
            None,
            Some(""),
            Some("sdk-cli"),
            Some("sdk-ts"),
            Some("claude-vscode"),
            Some("CLI"),
            Some(" cli"),
        ] {
            assert_eq!(
                classify(ep, Some(BUNDLED), Some(BUNDLED)),
                Origin::Unknown,
                "{ep:?}"
            );
        }
    }

    #[test]
    fn wire_round_trip() {
        for o in [Origin::Terminal, Origin::DesktopApp, Origin::Unknown] {
            assert_eq!(Origin::from_wire(Some(o.as_str())), o);
        }
        assert_eq!(Origin::from_wire(None), Origin::Unknown);
        assert_eq!(Origin::from_wire(Some("<script>")), Origin::Unknown);
    }

    #[test]
    fn labels() {
        assert_eq!(Origin::Terminal.ask_label(), "ターミナルで");
        assert_eq!(Origin::DesktopApp.ask_label(), "Claude アプリで");
        assert_eq!(Origin::Unknown.ask_label(), "Claude Code で");
        assert_eq!(
            Origin::Terminal.note(),
            "全文が見えないので、許可はターミナルで"
        );
        assert_eq!(
            Origin::DesktopApp.note(),
            "全文が見えないので、許可は Claude アプリで"
        );
        assert_eq!(
            Origin::Unknown.note(),
            "全文が見えないので、許可は Claude Code で"
        );
    }
}
