//! フォルダを選ぶダイアログ（設定画面の「作業フォルダ」。Windows の共通ダイアログ `IFileOpenDialog` を
//! `FOS_PICKFOLDERS` で開く）。
//!
//! Tauri のダイアログのプラグインを足さずに済ませる（依存を増やさない。既に `windows` crate で COM を使っている）。
//! ダイアログは閉じるまで戻らないので、呼ぶ側は別スレッド（`spawn_blocking`）から呼ぶ。

use std::path::PathBuf;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, CoUninitialize,
};
use windows::Win32::UI::Shell::{
    FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
};
use windows::core::HSTRING;

/// フォルダを選ぶダイアログを開き、選んだフォルダのパスを返す。取り消し・失敗は `None`。
///
/// `owner` は親にする窓のハンドル（設定の窓。あるとダイアログがその窓の前に出て、閉じるまで窓を触れない）。
/// このスレッドで COM を STA として初期化し、終わったら片づける（Tauri の worker スレッドは COM を
/// 初期化していない。既に初期化されていた時は `CoInitializeEx` が S_FALSE を返すので、その時も対で片づける）。
pub fn pick_folder(title: &str, owner: Option<isize>) -> Option<PathBuf> {
    // SAFETY: COM の呼び出し。ハンドルはこの関数の中で作って使い切る。owner は呼ぶ側が生きている窓のもの
    unsafe {
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if init.is_err() {
            return None;
        }
        let picked = show(title, owner.map(|h| HWND(h as *mut _)));
        CoUninitialize();
        picked
    }
}

/// ダイアログを出して選んだパスを返す（`pick_folder` の中身。COM は初期化済みの前提）。
unsafe fn show(title: &str, owner: Option<HWND>) -> Option<PathBuf> {
    // SAFETY: 呼ぶ側（pick_folder）が COM を初期化している。返る PWSTR は使ったら CoTaskMemFree で返す
    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let options = dialog.GetOptions().ok()?;
        // ファイルシステムのフォルダだけ（ライブラリや「PC」のような仮想の場所を返さない）
        dialog
            .SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)
            .ok()?;
        dialog.SetTitle(&HSTRING::from(title)).ok()?;
        // 取り消しは HRESULT_FROM_WIN32(ERROR_CANCELLED) のエラーで返る
        dialog.Show(owner).ok()?;
        let item = dialog.GetResult().ok()?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = name.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(name.0 as *const _));
        path
    }
}
