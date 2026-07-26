# Start Network Cartographer on Windows and clean up after exit.
$ErrorActionPreference = "Stop"

$architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
$releaseArchitecture = switch ($architecture) {
  "X64" { "x86_64" }
  "Arm64" { "arm64" }
  default { throw "This device is not supported yet." }
}

$repository = "jonbng/network-cartographer"
$package = "network-cartographer-windows-$releaseArchitecture"
$archive = "$package.zip"
$url = "https://github.com/$repository/releases/latest/download/$archive"
$workDirectory = Join-Path ([System.IO.Path]::GetTempPath()) ("network-cartographer-" + [guid]::NewGuid())
$previousProgressPreference = $ProgressPreference
$ranApp = $false

try {
  New-Item -ItemType Directory -Path $workDirectory | Out-Null
  $archivePath = Join-Path $workDirectory $archive

  Write-Host ""
  Write-Host "Network Cartographer"
  Write-Host "---------------------"
  Write-Host ("  {0,-12} {1}" -f "Status", "Starting")
  Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $archivePath
  $ProgressPreference = "SilentlyContinue"
  $checksumsPath = Join-Path $workDirectory "SHA256SUMS"
  Invoke-WebRequest -UseBasicParsing -Uri "https://github.com/$repository/releases/latest/download/SHA256SUMS" -OutFile $checksumsPath
  $checksumLine = Get-Content $checksumsPath | Where-Object { $_ -match "\s$([regex]::Escape($archive))$" } | Select-Object -First 1
  if (-not $checksumLine) {
    throw "Network Cartographer could not be verified. Please try again."
  }
  $expectedHash = ($checksumLine -split "\s+")[0].ToUpperInvariant()
  $actualHash = (Get-FileHash -Algorithm SHA256 $archivePath).Hash
  if ($actualHash -ne $expectedHash) {
    throw "Network Cartographer could not be verified. Please try again."
  }
  Expand-Archive -Path $archivePath -DestinationPath $workDirectory

  $binary = Join-Path $workDirectory "$package/netcart.exe"
  if (-not (Test-Path $binary)) {
    throw "Network Cartographer could not be started."
  }

  $hadLauncherFlag = Test-Path Env:NETCART_LAUNCHED
  $previousLauncherFlag = $env:NETCART_LAUNCHED
  try {
    $env:NETCART_LAUNCHED = "1"
    $ranApp = $true
    & $binary
  } finally {
    if ($hadLauncherFlag) {
      $env:NETCART_LAUNCHED = $previousLauncherFlag
    } else {
      Remove-Item Env:NETCART_LAUNCHED -ErrorAction SilentlyContinue
    }
  }
} finally {
  $ProgressPreference = $previousProgressPreference
  if (Test-Path $workDirectory) {
    Remove-Item -Recurse -Force $workDirectory
  }
  if ($ranApp) {
    Write-Host ("  {0,-12} {1}" -f "Cleanup", "temporary files removed")
  }
}

if ($ranApp) {
  Write-Host ("  {0,-12} {1}" -f "Done", "Network Cartographer stopped")
  Write-Host ""
}
