[CmdletBinding()]
param(
    [string]$InstallerPath,
    [string]$PreviousInstallerPath
)

$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($InstallerPath)) {
    $InstallerPath = Get-ChildItem (Join-Path $PSScriptRoot '..\src-tauri\target\release\bundle\nsis') -Filter '*-setup.exe' -File | Select-Object -First 1 -ExpandProperty FullName
}
if ([string]::IsNullOrWhiteSpace($InstallerPath) -or -not (Test-Path -LiteralPath $InstallerPath)) {
    throw 'No NSIS installer was found. Build it with pnpm tauri build --bundles nsis.'
}

$installRoot = Join-Path $env:TEMP ("deeppi-installer-smoke-{0}" -f $PID)
if (Test-Path -LiteralPath $installRoot) {
    Remove-Item -LiteralPath $installRoot -Recurse -Force
}
New-Item -ItemType Directory -Path $installRoot | Out-Null

function Get-InstalledExecutable {
    Get-ChildItem -LiteralPath $installRoot -Recurse -Filter 'deeppi.exe' -File | Select-Object -First 1
}

function Get-InstalledVersion($Executable) {
    $value = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($Executable.FullName).FileVersion
    if ([string]::IsNullOrWhiteSpace($value)) { return $null }
    try { return [Version]$value } catch { return $null }
}

function Invoke-Installer([string]$Path) {
    $arguments = @('/S', "/D=$installRoot")
    $process = Start-Process -FilePath $Path -ArgumentList $arguments -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "Installer failed with exit code $($process.ExitCode)."
    }
}

try {
    if (-not [string]::IsNullOrWhiteSpace($PreviousInstallerPath)) {
        if (-not (Test-Path -LiteralPath $PreviousInstallerPath)) {
            throw "Previous NSIS installer was not found: $PreviousInstallerPath"
        }
        Invoke-Installer $PreviousInstallerPath
        $previousInstalled = Get-InstalledExecutable
        if ($null -eq $previousInstalled) {
            throw 'Previous installer did not produce an installed DeepPi executable.'
        }
        $previousVersion = Get-InstalledVersion $previousInstalled
    }

    Invoke-Installer $InstallerPath
    $installed = Get-InstalledExecutable
    if ($null -eq $installed) {
        throw "DeepPi executable was not installed under $installRoot."
    }

    if ($null -ne $previousVersion) {
        $currentVersion = Get-InstalledVersion $installed
        if ($null -ne $currentVersion -and $currentVersion -lt $previousVersion) {
            throw "Installer downgraded DeepPi from $previousVersion to $currentVersion."
        }
        if ($null -ne $currentVersion) {
            Write-Host "installer version: $previousVersion -> $currentVersion"
        }
    }

    # Running the same exact installer again covers the repair/reinstall path.
    Invoke-Installer $InstallerPath
    $uninstaller = Get-ChildItem -LiteralPath $installRoot -Recurse -Filter 'uninstall.exe' -File | Select-Object -First 1
    if ($null -eq $uninstaller) {
        throw "Uninstaller was not installed under $installRoot."
    }

    $remove = Start-Process -FilePath $uninstaller.FullName -ArgumentList '/S' -Wait -PassThru
    if ($remove.ExitCode -ne 0) {
        throw "Uninstaller failed with exit code $($remove.ExitCode)."
    }
    if (Test-Path -LiteralPath $installed.FullName) {
        throw "DeepPi executable still exists after uninstall: $($installed.FullName)"
    }

    Write-Host "installer smoke: PASS"
}
finally {
    if (Test-Path -LiteralPath $installRoot) {
        Remove-Item -LiteralPath $installRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
