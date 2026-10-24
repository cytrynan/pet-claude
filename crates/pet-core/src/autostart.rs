//! 自動起動（Phase 4・PRD N5）。ログオンしたらペットを出す Windows タスクの中身を組み、
//! 登録済みのタスクを読み戻す。`schtasks.exe` を起こすのは pet-app（`autostart.rs`）で、ここは文字列だけ。
//!
//! タスクは `run-hidden.vbs` を通して pet-app を起こす（K の規範: 定期実行・常駐の起動は窓を出さない）。
//! リリース版の pet-app は窓の無い exe だが、デバッグ版は端末の窓を開くので、どちらでも同じ経路にする。

/// 登録するタスクの名前（タスク スケジューラのルート直下）
pub const TASK_NAME: &str = "pet-claude";

/// `run-hidden.vbs` に渡すログの名前（`~/.claude/logs/hidden/<名前>.log`）
const LOG_NAME: &str = "pet-claude";

pub struct TaskSpec<'a> {
    /// `DOMAIN\user`。このユーザーのログオンだけで起こす（全ユーザーにすると登録に管理者が要る）
    pub user_id: &'a str,
    /// `run-hidden.vbs` の絶対パス
    pub vbs: &'a str,
    /// 起こす pet-app.exe の絶対パス
    pub exe: &'a str,
    /// 試験では無効のタスクを作ってすぐ消す（ログオンで発火させない）
    pub enabled: bool,
}

/// タスク スケジューラの XML（`schtasks /Create /XML` に渡す）。
///
/// 引用符・改行・制御文字を含むパスと相対パスは、引数を割ったり別の場所を指したりするので断る。
pub fn task_xml(spec: &TaskSpec) -> Result<String, String> {
    if spec.user_id.trim().is_empty() || has_control(spec.user_id) {
        return Err("ユーザー名が分からない".into());
    }
    for (what, p) in [("run-hidden.vbs", spec.vbs), ("pet-app.exe", spec.exe)] {
        if !is_absolute(p) || p.contains('"') || has_control(p) {
            return Err(format!("{what} のパスを使えない: {p}"));
        }
    }
    let workdir = spec.exe.rsplit_once(['\\', '/']).map_or("", |(d, _)| d);
    let args = format!(r#"//B //Nologo "{}" {LOG_NAME} "{}""#, spec.vbs, spec.exe);
    // 既定から変えている所: 時間の上限なし（既定 72 時間で止まる）・電池でも動く・優先度 4（既定 7＝通常未満だと
    // ペットの描画が他の負荷に負ける）・二重に起こさない（pet-app 自身もミューテックスで 1 つにする）
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>pet-claude をログオン時に起動する（pet-claude の設定画面から登録）</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <UserId>{user}</UserId>
      <Delay>PT10S</Delay>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>{enabled}</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>4</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>%SystemRoot%\System32\wscript.exe</Command>
      <Arguments>{args}</Arguments>
      <WorkingDirectory>{workdir}</WorkingDirectory>
    </Exec>
  </Actions>
</Task>
"#,
        user = escape(spec.user_id),
        enabled = spec.enabled,
        args = escape(&args),
        workdir = escape(workdir),
    ))
}

/// 登録済みのタスクの XML から、起こしている exe を読む。
/// `run-hidden.vbs` 経由なら引数の最後の引用符の中、そうでなければ `Command` そのもの。
pub fn registered_exe(xml: &str) -> Option<String> {
    let args = element(xml, "Arguments").map(unescape).unwrap_or_default();
    if args.to_ascii_lowercase().contains("run-hidden.vbs") {
        let quoted: Vec<&str> = args.split('"').skip(1).step_by(2).collect();
        if quoted.len() >= 2 {
            return quoted.last().map(|s| (*s).to_string());
        }
    }
    let cmd = unescape(element(xml, "Command")?);
    let cmd = cmd.trim().trim_matches('"');
    (!cmd.is_empty()).then(|| cmd.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutostartView {
    /// 登録なし
    Off,
    /// いま動いている pet-app が登録されている
    On,
    /// 別の場所の pet-app（古いビルド・別の作業フォルダ）が登録されている
    OtherExe(String),
}

/// 設定画面に出す状態。パスは大文字小文字と `/`・`\` の違いを無視して比べる（Windows のパス）
pub fn view(registered: Option<&str>, current_exe: &str) -> AutostartView {
    let norm = |p: &str| p.replace('/', "\\").to_lowercase();
    match registered {
        None => AutostartView::Off,
        Some(r) if norm(r) == norm(current_exe) => AutostartView::On,
        Some(r) => AutostartView::OtherExe(r.to_string()),
    }
}

/// `schtasks /Query /XML` の出力を文字列にする。UTF-16LE（BOM の有無どちらも）と UTF-8 を受ける
pub fn decode_output(bytes: &[u8]) -> String {
    let utf16 = |b: &[u8]| {
        let (pairs, _) = b.as_chunks::<2>();
        let units: Vec<u16> = pairs.iter().map(|&c| u16::from_le_bytes(c)).collect();
        String::from_utf16_lossy(&units)
    };
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(rest);
    }
    // XML は `<` や空白（ASCII）で始まるので、BOM の無い UTF-16LE は 2 byte 目が 0 になる
    if bytes.len() >= 2 && bytes.len().is_multiple_of(2) && bytes[0] != 0 && bytes[1] == 0 {
        return utf16(bytes);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

/// `schtasks /Create /XML` に渡すファイルの中身（宣言どおり UTF-16LE＋BOM。日本語のパスでも化けない）
pub fn utf16le_with_bom(s: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    out.extend(s.encode_utf16().flat_map(u16::to_le_bytes));
    out
}

fn is_absolute(p: &str) -> bool {
    let b = p.as_bytes();
    let drive = b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\';
    drive || p.starts_with(r"\\")
}

fn has_control(s: &str) -> bool {
    s.chars().any(char::is_control)
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// 最初の `<name>…</name>` の中身（属性付きの開きタグも受ける）
fn element<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let open = xml.find(&format!("<{name}"))?;
    let start = open + xml[open..].find('>')? + 1;
    let end = start + xml[start..].find(&format!("</{name}>"))?;
    Some(&xml[start..end])
}
