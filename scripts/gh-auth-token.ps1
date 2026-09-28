#Requires -Version 5.1
<#
  gh-auth-token.ps1 — 无浏览器登录 GitHub CLI (方案 A)

  为什么需要它:
    * `gh auth login` 的浏览器设备码流程要求浏览器里"已经登录"github.com,
      否则 github.com/login/device 会 302 跳回登录页 —— 你遇到的"收不到验证码"就是这个。
    * 用 Personal Access Token (PAT) 可以完全绕过浏览器。
    * 直接 `gh auth login` 粘贴 token 的交互界面在非交互终端里会卡死,
      所以本脚本走官方的 `gh auth login --with-token` (stdin 管道),这是给自动化用的正规通道。

  token 不会写入本聊天/会话记录,也不会留在磁盘上。
#>

[CmdletBinding()]
param(
    # 允许直接传 token(不推荐:会进入命令历史)。默认用隐藏输入提示。
    [string]$Token,
    # 登录后自动跑一遍完整校验向导,并衔接 bash 版 wizard
    [switch]$ThenWizard
)

$ErrorActionPreference = 'Stop'

function Write-Head($t)  { Write-Host ""; Write-Host "  $t" -ForegroundColor Cyan -BackgroundColor Black }
function Write-Ok($t)    { Write-Host "  [OK] $t"   -ForegroundColor Green }
function Write-Warn2($t) { Write-Host "  [!]  $t"   -ForegroundColor Yellow }
function Write-Info($t)  { Write-Host "      $t"    -ForegroundColor DarkGray }

# ── 1. 找到 gh ───────────────────────────────────────────────────────────
Write-Head "1/5  定位 gh"

$ghExe = $null
$onPath = Get-Command gh -ErrorAction SilentlyContinue
if ($onPath) { $ghExe = $onPath.Source }

if (-not $ghExe) {
    $candidates = @(
        (Join-Path $env:LOCALAPPDATA 'Programs\gh\bin\gh.exe'),
        (Join-Path $env:ProgramFiles 'GitHub CLI\gh.exe'),
        'C:\ProgramData\chocolatey\bin\gh.exe'
    )
    $ghExe = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}

if (-not $ghExe) {
    Write-Warn2 "找不到 gh.exe。先安装:winget install --id GitHub.cli --exact"
    exit 1
}
Write-Ok "找到 $ghExe"
Write-Info (& $ghExe --version | Select-Object -First 1)

# ── 2. 已经有登录态? ──────────────────────────────────────────────────────
Write-Head "2/5  检查当前登录状态"

$already = $false
try {
    & $ghExe auth status *> $null
    $already = ($LASTEXITCODE -eq 0)
} catch { $already = $false }

if ($already) {
    Write-Ok "已经登录了,无需重复写入 token"
    & $ghExe auth status 2>&1 | ForEach-Object { Write-Info $_ }
    Write-Head "完成"
    exit 0
}
Write-Info "尚未登录 —— 继续"

# ── 3. 取得 token ────────────────────────────────────────────────────────
Write-Head "3/5  取得 token"

if (-not $Token) {
    Write-Host "  仓库 springkai66/deep-pi 是 public,所以只需要很小的权限。"
    Write-Host ""
    Write-Host "  点这个链接(scope 已预填,直接拉到页面底部点 Generate 即可):" -ForegroundColor White
    Write-Host "    https://github.com/settings/tokens/new?scopes=public_repo,read:org&description=gh-cli-deep-pi" -ForegroundColor Green
    Write-Host ""
    Write-Info "public_repo = 读写公开仓库的 issue / PR / 内容"
    Write-Info "read:org    = 读取组织信息(gh 部分命令需要)"
    Write-Info "不需要 workflow scope,除非以后你要用 gh 改 .github/workflows 里的文件"
    Write-Host ""
    Write-Host "  建议勾选有效期(例如 90 天),比永久 token 安全。" -ForegroundColor DarkGray
    Write-Host ""

    try { Start-Process 'https://github.com/settings/tokens/new?scopes=public_repo,read:org&description=gh-cli-deep-pi' } catch { }
    Write-Host "  已尝试打开浏览器;打不开就手动复制上面的链接。" -ForegroundColor DarkGray
    Write-Host ""
    Write-Host "  把生成的 token 粘贴到这里(输入是隐藏的,不会回显):" -ForegroundColor White

    $secure = Read-Host -Prompt "  token" -AsSecureString
    $bstr   = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
    try   { $Token = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr) }
    finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr) }
}

$Token = $Token.Trim()
if ([string]::IsNullOrWhiteSpace($Token)) {
    Write-Warn2 "没有输入 token,退出。"
    exit 1
}

# 粗校验前缀,尽早发现粘错
if ($Token -notmatch '^(gh[pousr]_|github_pat_)') {
    Write-Warn2 "这个值看起来不像 GitHub token(通常以 ghp_ / gho_ / ghu_ / ghs_ / ghr_ / github_pat_ 开头)。"
    Write-Warn2 "如果你复制错了内容,按 Ctrl-C 退出重来。"
    if (-not $ThenWizard) {
        $go = Read-Host "  仍然继续?[y/N]"
        if ($go -notmatch '^[Yy]') { exit 1 }
    }
} else {
    Write-Ok "token 格式看起来正常($($Token.Substring(0,[Math]::Min(7,$Token.Length)))…,共 $($Token.Length) 字符)"
}

# ── 4. 写入 gh ───────────────────────────────────────────────────────────
Write-Head "4/5  用 --with-token 写入"

$tmp = Join-Path $env:TEMP ("ghtok_{0}.tmp" -f ([guid]::NewGuid().ToString('N')))
try {
    # 无 BOM、无尾随换行 —— 避免 token 被污染
    [IO.File]::WriteAllText($tmp, $Token, (New-Object Text.UTF8Encoding($false)))

    # 必须走 --with-token + stdin,这才是官方给自动化的通道。
    # 注意:PowerShell 没有 `<` 输入重定向,所以借 cmd.exe 做文件->stdin,
    # 同时避免 PowerShell 管道给 token 加上 BOM / CRLF 污染。
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName               = $env:ComSpec          # cmd.exe
    $psi.Arguments              = '/c ""{0}" auth login --with-token < "{1}""' -f $ghExe, $tmp
    $psi.UseShellExecute        = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError  = $true
    $psi.CreateNoWindow         = $true

    $proc = [System.Diagnostics.Process]::Start($psi)
    $stdout = $proc.StandardOutput.ReadToEnd()
    $stderr = $proc.StandardError.ReadToEnd()
    $proc.WaitForExit()
    $code = $proc.ExitCode
    $out  = @($stdout, $stderr) | Where-Object { $_ }

    if ($code -eq 0) {
        Write-Ok "token 已写入 gh"
    } else {
        Write-Warn2 "写入失败(exit $code)"
        $out | ForEach-Object { Write-Info $_ }
        Write-Info "常见原因:token 已过期 / 复制时多带了字符 / scope 不足"
        exit 1
    }
}
finally {
    if (Test-Path $tmp) { Remove-Item $tmp -Force }
    Write-Info "临时文件已删除"
}

# ── 5. 校验 ──────────────────────────────────────────────────────────────
Write-Head "5/5  校验"

$null = & $ghExe auth status *> $null
if ($LASTEXITCODE -ne 0) {
    Write-Warn2 "auth status 仍然失败"
    & $ghExe auth status 2>&1 | ForEach-Object { Write-Info $_ }
    exit 1
}

& $ghExe auth status 2>&1 | ForEach-Object { Write-Info $_ }

$login = $null
try { $login = (& $ghExe api user --jq .login 2>$null) } catch { }
if ($login) { Write-Ok "身份: $login" }

Write-Host ""
Write-Host "  实测一次真实 API 调用:" -ForegroundColor Cyan
try {
    & $ghExe api repos/springkai66/deep-pi --jq '"  \(.full_name)  visibility=\(.visibility)  stars=\(.stargazers_count)"' 2>&1 |
        ForEach-Object { Write-Host $_ -ForegroundColor Green }
    Write-Ok "token 可用"
} catch {
    Write-Warn2 "API 调用失败,检查网络或代理"
}

Write-Host ""
Write-Host "  常用命令:" -ForegroundColor White
Write-Info "gh run list --repo springkai66/deep-pi     # 看 CI"
Write-Info "gh issue list --repo springkai66/deep-pi"
Write-Info "gh pr status"
Write-Host ""
Write-Warn2 "token 只存在 gh 的凭据里;聊天记录里没有,临时文件也已删除。"
Write-Host ""

if ($ThenWizard) {
    $bash = 'C:\Program Files\Git\bin\bash.exe'
    if (Test-Path $bash) {
        Write-Head "衔接:运行完整校验向导"
        & $bash 'F:\deep-pi\scripts\gh-cli-setup.sh'
    }
}
