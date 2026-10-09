param(
    [string]$ProjectRoot = (Join-Path $PSScriptRoot '..'),
    [string]$OutputDirectory = ''
)

$ErrorActionPreference = 'Stop'
$ProjectRoot = (Resolve-Path -LiteralPath $ProjectRoot).Path
$version = (Get-Content -LiteralPath (Join-Path $ProjectRoot 'package.json') -Raw | ConvertFrom-Json).version
$source = Join-Path $ProjectRoot 'src-tauri/target/release/deeppi.exe'
$info = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($source)
if ($info.ProductVersion -ne $version) {
    throw "Portable executable version $($info.ProductVersion) does not match $version."
}

$stream = [System.IO.File]::OpenRead($source)
$reader = [System.IO.BinaryReader]::new($stream)
try {
    if ($reader.ReadUInt16() -ne 0x5a4d) { throw 'Invalid DOS executable signature.' }
    $stream.Position = 0x3c
    $peOffset = $reader.ReadInt32()
    $stream.Position = $peOffset
    if ($reader.ReadUInt32() -ne 0x4550) { throw 'Invalid PE executable signature.' }
    if ($reader.ReadUInt16() -ne 0x8664) { throw 'Portable executable must be Windows x64.' }
} finally {
    $reader.Dispose()
    $stream.Dispose()
}

if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $ProjectRoot 'src-tauri/target/release/bundle/portable'
}
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$fileName = "DeepPi_${version}_x64-portable.exe"
$destination = Join-Path $OutputDirectory $fileName
Copy-Item -LiteralPath $source -Destination $destination -Force
$hash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash
if ($hash -ne (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash) {
    throw 'Portable executable checksum differs from the built application.'
}
$utf8 = [System.Text.UTF8Encoding]::new($false)
[System.IO.File]::WriteAllText("$destination.sha256", "$hash  $fileName`n", $utf8)
$readme = @"
DeepPi $version — Windows x64 免安装 EXE

将 $fileName 下载到当前用户可写的文件夹，双击运行，无需安装 DeepPi。
系统需已有 Microsoft Edge WebView2 Runtime：
https://developer.microsoft.com/microsoft-edge/webview2/

Pi / DeepSeek Harness 运行时可在应用中按需下载。
运行时存放在 EXE 同级的 runtimes 目录；移动程序时请一并移动该目录。
设置和用户数据仍保存在当前 Windows 用户的 AppData 中。
"@
[System.IO.File]::WriteAllText((Join-Path $OutputDirectory 'PORTABLE_README.txt'), $readme, $utf8)
Write-Host "Verified portable EXE: $destination ($($info.ProductVersion), x64)"
Write-Host "SHA256: $hash"
