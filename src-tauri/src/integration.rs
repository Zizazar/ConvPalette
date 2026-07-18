//! Интеграция с Windows: пункты контекстного меню Проводника и автозапуск.
//! Всё пишется в HKCU — без прав администратора, полностью обратимо.

#[cfg(target_os = "windows")]
mod win {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    use winreg::RegKey;

    const VERB: &str = "ConvPalette";
    const MENU_TITLE: &str = "Конвертировать (ConvPalette)";
    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const RUN_VALUE: &str = "ConvPalette";

    /// Расширения, для которых показывать пункт меню (медиафайлы).
    const EXTENSIONS: &[&str] = &[
        ".mp4", ".mov", ".mkv", ".avi", ".webm", ".flv", ".m4v", ".wmv", ".mpg", ".mpeg", ".ts",
        ".3gp", ".mp3", ".wav", ".flac", ".m4a", ".aac", ".ogg", ".opus", ".wma", ".aiff", ".png",
        ".jpg", ".jpeg", ".webp", ".bmp", ".gif", ".tiff", ".tif", ".heic", ".heif",
    ];

    fn current_exe() -> Result<String, String> {
        std::env::current_exe()
            .map_err(|e| e.to_string())
            .map(|p| p.to_string_lossy().to_string())
    }

    /// Базовые ключи, под которыми создаётся `shell\ConvPalette`.
    fn base_keys() -> Vec<(String, &'static str)> {
        let mut keys: Vec<(String, &'static str)> = EXTENSIONS
            .iter()
            .map(|ext| {
                (
                    format!(r"Software\Classes\SystemFileAssociations\{ext}"),
                    "%1",
                )
            })
            .collect();
        keys.push((r"Software\Classes\Directory".into(), "%1"));
        keys.push((r"Software\Classes\Directory\Background".into(), "%V"));
        keys
    }

    fn set_verb(hkcu: &RegKey, base: &str, exe: &str, arg_token: &str) -> Result<(), String> {
        let shell_path = format!(r"{base}\shell\{VERB}");
        let (shell, _) = hkcu.create_subkey(&shell_path).map_err(|e| e.to_string())?;
        shell
            .set_value("", &MENU_TITLE)
            .map_err(|e| e.to_string())?;
        shell.set_value("Icon", &exe).map_err(|e| e.to_string())?;
        let (cmd, _) = hkcu
            .create_subkey(format!(r"{shell_path}\command"))
            .map_err(|e| e.to_string())?;
        cmd.set_value("", &format!("\"{exe}\" \"{arg_token}\""))
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Зарегистрировать пункты меню на текущий exe.
    pub fn register_context_menu() -> Result<(), String> {
        let exe = current_exe()?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        for (base, token) in base_keys() {
            set_verb(&hkcu, &base, &exe, token)?;
        }
        Ok(())
    }

    /// Удалить пункты меню.
    pub fn unregister_context_menu() -> Result<(), String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        for (base, _) in base_keys() {
            // Отсутствие ключа — не ошибка (например, уже удалён).
            let _ = hkcu.delete_subkey_all(format!(r"{base}\shell\{VERB}"));
        }
        Ok(())
    }

    /// Зарегистрировано ли меню (проверяем по ключу для папок).
    pub fn context_menu_registered() -> bool {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(
                format!(r"Software\Classes\Directory\shell\{VERB}\command"),
                KEY_READ,
            )
            .is_ok()
    }

    /// Включить/выключить автозапуск при входе в Windows.
    pub fn set_autostart(enabled: bool) -> Result<(), String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (run, _) = hkcu.create_subkey(RUN_KEY).map_err(|e| e.to_string())?;
        if enabled {
            let exe = current_exe()?;
            run.set_value(RUN_VALUE, &format!("\"{exe}\""))
                .map_err(|e| e.to_string())?;
        } else {
            let _ = run.delete_value(RUN_VALUE);
        }
        Ok(())
    }

    pub fn autostart_enabled() -> bool {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(RUN_KEY, KEY_READ)
            .and_then(|k| k.get_value::<String, _>(RUN_VALUE))
            .is_ok()
    }
}

#[cfg(target_os = "windows")]
pub use win::*;

// Заглушки для не-Windows (dev-сборки на других ОС).
#[cfg(not(target_os = "windows"))]
mod stub {
    pub fn register_context_menu() -> Result<(), String> {
        Err("Контекстное меню поддерживается только в Windows".into())
    }
    pub fn unregister_context_menu() -> Result<(), String> {
        Ok(())
    }
    pub fn context_menu_registered() -> bool {
        false
    }
    pub fn set_autostart(_enabled: bool) -> Result<(), String> {
        Err("Автозапуск поддерживается только в Windows".into())
    }
    pub fn autostart_enabled() -> bool {
        false
    }
}

#[cfg(not(target_os = "windows"))]
pub use stub::*;
