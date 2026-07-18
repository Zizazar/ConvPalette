<#
.SYNOPSIS
  Регистрирует пункты контекстного меню Проводника для ConvPalette (в HKCU — без прав администратора).

.DESCRIPTION
  Добавляет пункт «Конвертировать (ConvPalette)» для:
    - выбранных медиафайлов (по списку расширений);
    - папки (правый клик на папке);
    - фона папки (правый клик внутри открытой папки).
  При вызове передаёт путь(и) в приложение; множественный выбор агрегируется
  через single-instance внутри приложения.

.PARAMETER ExePath
  Путь к ConvPalette.exe. Если не указан — ищется release-, затем debug-сборка.
#>
param(
  [string]$ExePath
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

if (-not $ExePath) {
  $candidates = @(
    (Join-Path $root "src-tauri\target\release\ConvPalette.exe"),
    (Join-Path $root "src-tauri\target\release\cta-conv.exe"),
    (Join-Path $root "src-tauri\target\debug\ConvPalette.exe"),
    (Join-Path $root "src-tauri\target\debug\cta-conv.exe")
  )
  $ExePath = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}

if (-not $ExePath -or -not (Test-Path $ExePath)) {
  Write-Error "Не найден ConvPalette.exe. Соберите приложение (npm run tauri build) или укажите -ExePath."
  exit 1
}

$ExePath = (Resolve-Path $ExePath).Path
Write-Host "Использую бинарник: $ExePath"

$menuTitle = "Конвертировать (ConvPalette)"
$verbKey   = "ConvPalette"

# Расширения, для которых показывать пункт (медиа).
$extensions = @(
  ".mp4",".mov",".mkv",".avi",".webm",".flv",".m4v",".wmv",".mpg",".mpeg",".ts",".3gp",
  ".mp3",".wav",".flac",".m4a",".aac",".ogg",".opus",".wma",".aiff",
  ".png",".jpg",".jpeg",".webp",".bmp",".gif",".tiff",".tif",".heic",".heif"
)

function Set-Verb($basePath, $argToken) {
  $shellKey = "$basePath\shell\$verbKey"
  New-Item -Path $shellKey -Force | Out-Null
  New-ItemProperty -Path $shellKey -Name "(default)"     -Value $menuTitle -PropertyType String -Force | Out-Null
  New-ItemProperty -Path $shellKey -Name "Icon"          -Value $ExePath   -PropertyType String -Force | Out-Null
  $cmdKey = "$shellKey\command"
  New-Item -Path $cmdKey -Force | Out-Null
  New-ItemProperty -Path $cmdKey -Name "(default)" -Value "`"$ExePath`" `"$argToken`"" -PropertyType String -Force | Out-Null
}

# Файлы по расширениям.
foreach ($ext in $extensions) {
  Set-Verb "HKCU:\Software\Classes\SystemFileAssociations\$ext" "%1"
}

# Папка и фон папки.
Set-Verb "HKCU:\Software\Classes\Directory" "%1"
Set-Verb "HKCU:\Software\Classes\Directory\Background" "%V"

Write-Host "Готово. Пункт «$menuTitle» добавлен в контекстное меню."
Write-Host "В Windows 11 он может быть под «Показать дополнительные параметры»."
