//! 端末の窓を出さない GUI サブシステムのまま、CLI の引数（`--probe` など）の時だけ親の端末に出力する。
//!
//! なぜ: ペットは手で起こしても自動起動でも黒い窓を見せない（K 2026-10-26）。ただし GUI サブシステムの
//! プロセスは端末から起こしても標準出力が端末につながらないので、`--probe` の結果が見えなくなる。
//! そこで CLI の引数がある時だけ `AttachConsole(ATTACH_PARENT_PROCESS)` で親（PowerShell など）の端末に
//! つなぎ直す。親に端末が無い（タスク スケジューラ・ダブルクリック）時は失敗するが、その時は出す先も無いので放っておく。

use windows::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_OUTPUT_HANDLE,
};

/// 標準出力に結果を書いて終わる引数。ここに無い起動は常駐なので端末につながない
const CLI_FLAGS: [&str; 6] = [
    "--probe",
    "--install-hooks",
    "--uninstall-hooks",
    "--install-autostart",
    "--uninstall-autostart",
    "--autostart-status",
];

/// 引数に CLI のものが 1 つでもあれば true（`--settings <path>` のような値だけの引数は数えない）
pub fn is_cli_mode(args: &[String]) -> bool {
    args.iter().any(|a| CLI_FLAGS.contains(&a.as_str()))
}

/// CLI の引数の時だけ親の端末につなぐ。常駐の起動では何もしない（つなぐと黒い窓が出る理由になる）。
///
/// 標準出力が既にある時（`> file` やパイプで起こされた＝検証スクリプトや gate）はつながない。
/// `AttachConsole` は標準出力を端末に付け替えるので、つなぐとリダイレクト先に何も届かなくなる
/// （2026-10-26 実測: `cmd /c "pet-app --autostart-status > out.txt"` が空になった）
pub fn attach_parent_if_cli(args: &[String]) {
    if !is_cli_mode(args) {
        return;
    }
    // SAFETY: 読むだけ。GUI サブシステムのプロセスは、親が付け替えていなければ標準出力を持たない（NULL）
    let has_stdout = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) }.is_ok_and(|h| !h.is_invalid());
    if has_stdout {
        return;
    }
    // SAFETY: 引数は定数。失敗（親に端末が無い）は無視してよい
    let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn resident_launch_is_not_cli() {
        assert!(!is_cli_mode(&args(&["pet-app.exe"])));
        assert!(!is_cli_mode(&args(&[
            "pet-app.exe",
            "--pet",
            "assets/pets/dragon"
        ])));
        assert!(!is_cli_mode(&args(&["pet-app.exe", "--open-settings"])));
    }

    #[test]
    fn every_stdout_flag_is_cli() {
        for f in CLI_FLAGS {
            assert!(is_cli_mode(&args(&["pet-app.exe", f])), "{f}");
        }
        assert!(is_cli_mode(&args(&[
            "pet-app.exe",
            "--install-hooks",
            "--settings",
            "x.json"
        ])));
    }

    #[test]
    fn a_bare_word_is_not_a_flag() {
        // 完全一致だけ。`probe` や `--probe=1` のような似た文字列ではつながない
        assert!(!is_cli_mode(&args(&["pet-app.exe", "--pet", "probe"])));
        assert!(!is_cli_mode(&args(&["pet-app.exe", "--probe=1"])));
    }
}
