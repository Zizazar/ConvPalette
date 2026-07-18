; Хуки NSIS-инсталлятора ConvPalette (см. tauri.conf.json → bundle.windows.nsis.installerHooks).
; Установка идёт в профиль пользователя (currentUser), поэтому все ключи — HKCU.
; После деинсталляции подчищаем то, что приложение пишет в реестр само
; (тумблеры «контекстное меню» и «автозапуск» в настройках, integration.rs).

!macro NSIS_HOOK_PREINSTALL
!macroend

!macro NSIS_HOOK_POSTINSTALL
!macroend

!macro NSIS_HOOK_PREUNINSTALL
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Пункт контекстного меню Проводника: файлы по расширениям
  ; (список должен совпадать с EXTENSIONS в src-tauri/src/integration.rs)
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.mp4\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.mov\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.mkv\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.avi\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.webm\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.flv\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.m4v\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.wmv\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.mpg\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.mpeg\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.ts\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.3gp\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.mp3\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.wav\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.flac\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.m4a\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.aac\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.ogg\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.opus\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.wma\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.aiff\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.png\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.jpg\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.jpeg\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.webp\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.bmp\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.gif\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.tiff\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.tif\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.heic\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.heif\shell\ConvPalette"
  ; Пункт для папок и фона папки
  DeleteRegKey HKCU "Software\Classes\Directory\shell\ConvPalette"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\ConvPalette"
  ; Автозапуск
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "ConvPalette"
!macroend
