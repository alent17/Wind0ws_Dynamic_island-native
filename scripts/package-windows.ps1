param([string]$MakensisPath)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$manifest = Join-Path $workspace 'native\Cargo.toml'
$version = [regex]::Match([IO.File]::ReadAllText($manifest), 'version = "([^"]+)"').Groups[1].Value
$target = Join-Path $workspace 'native\target'
& cargo build --release --manifest-path $manifest --target-dir $target --bin isle-native
if ($LASTEXITCODE -ne 0) { throw 'Native release build failed' }
$stage = Join-Path $workspace "dist\native-$version"
New-Item -ItemType Directory -Path (Join-Path $stage 'fonts') -Force | Out-Null
$executable = Join-Path $target 'release\isle-native.exe'
if ((Get-Item -LiteralPath $executable).VersionInfo.ProductVersion -ne $version) { throw 'Native executable version mismatch' }
Copy-Item -LiteralPath $executable -Destination (Join-Path $stage 'isle.exe') -Force
foreach ($weight in @('Regular', 'Medium', 'Bold')) {
    Copy-Item -LiteralPath (Join-Path $target "release\fonts\MiSans-$weight.ttf") -Destination (Join-Path $stage 'fonts') -Force
}
Copy-Item -LiteralPath (Join-Path $workspace 'LICENSE') -Destination (Join-Path $stage 'LICENSE') -Force
Copy-Item -LiteralPath (Join-Path $target 'release\LUCIDE-LICENSE.txt') -Destination $stage -Force
if (-not $MakensisPath) {
    $command = Get-Command makensis.exe -ErrorAction SilentlyContinue
    if ($command) { $MakensisPath = $command.Source }
    else {
        $candidates = @((Join-Path ${env:ProgramFiles(x86)} 'NSIS\makensis.exe'), (Join-Path $env:LOCALAPPDATA 'tauri\NSIS\makensis.exe'))
        $MakensisPath = $candidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
    }
}
if (-not $MakensisPath) { throw 'Install NSIS or pass -MakensisPath to build the installer' }
$installer = Join-Path $workspace "dist\Isle_${version}_x64-setup.exe"
& $MakensisPath /V2 "/DVERSION=$version" "/DSTAGE=$stage" "/DOUTPUT=$installer" "/DICON=$(Join-Path $workspace 'src-tauri\icons\icon.ico')" (Join-Path $PSScriptRoot 'installer.nsi')
if ($LASTEXITCODE -ne 0) { throw 'Native installer build failed' }
$portable = Join-Path $workspace "dist\Isle_${version}_native_x64.zip"
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $portable -Force
foreach ($file in @($installer, $portable)) {
    $stream = [IO.File]::OpenRead($file)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $hash = ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-', '') }
    finally { $stream.Dispose(); $sha.Dispose() }
    Write-Output "$hash  $file"
}
