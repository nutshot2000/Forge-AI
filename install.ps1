# Builds forge and installs it into .\app, which is what the Forge shortcut and the
# Claude MCP registration run. Development builds in .\target never touch the running app,
# so you can rebuild any time; run this script to update the installed copy.
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

cargo build --release
if ($LASTEXITCODE -ne 0) { throw "build failed" }

$app = Join-Path $PSScriptRoot 'app'
New-Item -ItemType Directory -Force $app | Out-Null
# A running exe can't be overwritten but can be renamed: move it aside so Forge (or a Claude
# session) keeps running on the old copy, and the next launch picks up the new one.
Get-ChildItem $app -Filter '*.old-*.exe' -ErrorAction SilentlyContinue | Remove-Item -ErrorAction SilentlyContinue
foreach ($exe in 'forge.exe', 'forge-app.exe') {
    $dest = Join-Path $app $exe
    if (Test-Path $dest) {
        try { Remove-Item $dest -ErrorAction Stop }
        catch { Rename-Item $dest ("{0}.old-{1}.exe" -f [IO.Path]::GetFileNameWithoutExtension($exe), (Get-Date -Format 'yyyyMMddHHmmss')) }
    }
    Copy-Item (Join-Path 'target\release' $exe) $dest
}

# Shortcuts on the Desktop and in the Start menu.
$ws = New-Object -ComObject WScript.Shell
foreach ($dir in @([Environment]::GetFolderPath('Desktop'), [Environment]::GetFolderPath('Programs'))) {
    $lnk = $ws.CreateShortcut((Join-Path $dir 'Forge.lnk'))
    $lnk.TargetPath = Join-Path $app 'forge-app.exe'
    $lnk.WorkingDirectory = $PSScriptRoot
    $lnk.IconLocation = (Join-Path $PSScriptRoot 'assets\forge.ico') + ',0'
    $lnk.Description = 'Forge game engine editor'
    $lnk.Save()
}
Write-Host "Installed Forge to $app" -ForegroundColor Green
