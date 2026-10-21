//! チャットの API キーを Windows の資格情報マネージャーに置く（K 決定 2026-10-21 Q2）。
//! `state.json` には「API キーを使うか」だけを書き、鍵そのものはここにだけ置く（N4）。
//! 種類は GENERIC、名前は `pet_core::chat::CREDENTIAL_TARGET`、保存先はログオンをまたいで残る
//! ローカルの資格情報（自分のユーザーだけが読める）。鍵の値はログにもエラーの文にも入れない。

use pet_core::chat::CREDENTIAL_TARGET;
use windows::Win32::Security::Credentials::{
    CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
    CredReadW, CredWriteW,
};
use windows::core::{HSTRING, PWSTR};

/// API キーを保存する（同じ名前があれば上書き）。前後の空白は落とす。空なら保存しない
pub fn save_key(key: &str) -> Result<(), String> {
    write(CREDENTIAL_TARGET, key.trim())
}

/// 保存した API キー。無い・読めないなら `None`
pub fn load_key() -> Option<String> {
    read(CREDENTIAL_TARGET)
}

/// 鍵が保存されているか（設定画面の「保存済み」の表示用。値は返さない）
pub fn has_key() -> bool {
    load_key().is_some()
}

/// 保存した API キーを消す。もともと無いなら成功扱い
pub fn delete_key() -> Result<(), String> {
    delete(CREDENTIAL_TARGET)
}

fn write(target: &str, secret: &str) -> Result<(), String> {
    if secret.is_empty() {
        return Err("API キーが空です".into());
    }
    let mut name: Vec<u16> = target.encode_utf16().chain(Some(0)).collect();
    let mut blob = secret.as_bytes().to_vec();
    let cred = CREDENTIALW {
        Flags: CRED_FLAGS(0),
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(name.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..Default::default()
    };
    // SAFETY: cred の指す name・blob はこの関数の終わりまで生きている。CredWriteW は内容を複写する
    let r = unsafe { CredWriteW(&cred, 0) };
    // 手元の写しを消す（最善の努力。コンパイラが消す書き込みを省かないよう volatile で書く）
    for b in blob.iter_mut() {
        // SAFETY: blob の有効な要素への書き込み
        unsafe { std::ptr::write_volatile(b, 0) };
    }
    // エラーの文に鍵は入らない（HRESULT の説明だけ）
    r.map_err(|e| format!("資格情報マネージャーに保存できません: {}", e.message()))
}

fn read(target: &str) -> Option<String> {
    let mut p: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: 成功すれば p は OS が確保した CREDENTIALW を指し、CredFree で返す
    unsafe { CredReadW(&HSTRING::from(target), CRED_TYPE_GENERIC, None, &mut p) }.ok()?;
    if p.is_null() {
        return None;
    }
    // SAFETY: p は CredReadW が返した有効なポインタ。blob は CredentialBlobSize バイト
    let value = unsafe {
        let c = &*p;
        let bytes = if c.CredentialBlob.is_null() {
            &[][..]
        } else {
            std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize)
        };
        let s = String::from_utf8(bytes.to_vec()).ok();
        CredFree(p as *const _);
        s
    };
    value.filter(|s| !s.trim().is_empty())
}

fn delete(target: &str) -> Result<(), String> {
    // SAFETY: 名前の文字列は呼び出しの間生きている
    match unsafe { CredDeleteW(&HSTRING::from(target), CRED_TYPE_GENERIC, None) } {
        Ok(()) => Ok(()),
        // ERROR_NOT_FOUND（0x80070490）: もともと無い
        Err(e) if e.code().0 as u32 == 0x8007_0490 => Ok(()),
        Err(e) => Err(format!(
            "資格情報マネージャーから消せません: {}",
            e.message()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本物の資格情報マネージャーで、試験用の名前に書いて読んで消す（K の鍵の名前には触らない）
    #[test]
    fn write_read_delete_roundtrip() {
        let target = format!("pet-claude/test-{}", std::process::id());
        let secret = "sk-ant-test-値-0123";
        write(&target, secret).expect("書けること");
        assert_eq!(read(&target).as_deref(), Some(secret));
        write(&target, "sk-ant-test-2").expect("上書きできること");
        assert_eq!(read(&target).as_deref(), Some("sk-ant-test-2"));
        delete(&target).expect("消せること");
        assert_eq!(read(&target), None);
        // 無いものを消しても成功
        delete(&target).expect("無くても成功");
    }

    #[test]
    fn empty_secret_is_rejected() {
        let target = format!("pet-claude/test-empty-{}", std::process::id());
        assert!(write(&target, "").is_err());
        assert_eq!(read(&target), None);
    }
}
