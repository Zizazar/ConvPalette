<#
.SYNOPSIS
  Удаляет пункты контекстного меню ConvPalette, добавленные install-context-menu.ps1.
#>
$ErrorActionPreference = "SilentlyContinue"
$verbKey = "ConvPalette"

$extensions = @(
  ".mp4",".mov",".mkv",".avi",".webm",".flv",".m4v",".wmv",".mpg",".mpeg",".ts",".3gp",
  ".mp3",".wav",".flac",".m4a",".aac",".ogg",".opus",".wma",".aiff",
  ".png",".jpg",".jpeg",".webp",".bmp",".gif",".tiff",".tif",".heic",".heif"
)

foreach ($ext in $extensions) {
  Remove-Item -Path "HKCU:\Software\Classes\SystemFileAssociations\$ext\shell\$verbKey" -Recurse -Force
}
Remove-Item -Path "HKCU:\Software\Classes\Directory\shell\$verbKey" -Recurse -Force
Remove-Item -Path "HKCU:\Software\Classes\Directory\Background\shell\$verbKey" -Recurse -Force

Write-Host "Пункты контекстного меню ConvPalette удалены."
