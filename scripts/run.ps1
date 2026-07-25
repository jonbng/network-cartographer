# Download the latest Network Cartographer release for Windows and run it.
$ErrorActionPreference = "Stop"

$architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
if ($architecture -ne "X64") {
  throw "Network Cartographer currently supports Windows x64. Detected: $architecture"
}

$repository = "jonbng/network-cartographer"
$archive = "network-cartographer-windows-x86_64.zip"
$url = "https://github.com/$repository/releases/latest/download/$archive"
$workDirectory = Join-Path ([System.IO.Path]::GetTempPath()) ("network-cartographer-" + [guid]::NewGuid())

try {
  New-Item -ItemType Directory -Path $workDirectory | Out-Null
  $archivePath = Join-Path $workDirectory $archive

  Write-Host "Downloading Network Cartographer for Windows x64..."
  Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $archivePath
  $checksumsPath = Join-Path $workDirectory "SHA256SUMS"
  Invoke-WebRequest -UseBasicParsing -Uri "https://github.com/$repository/releases/latest/download/SHA256SUMS" -OutFile $checksumsPath
  $checksumLine = Get-Content $checksumsPath | Where-Object { $_ -match "\s$([regex]::Escape($archive))$" } | Select-Object -First 1
  if (-not $checksumLine) {
    throw "No checksum was published for $archive."
  }
  $expectedHash = ($checksumLine -split "\s+")[0].ToUpperInvariant()
  $actualHash = (Get-FileHash -Algorithm SHA256 $archivePath).Hash
  if ($actualHash -ne $expectedHash) {
    throw "The downloaded release failed checksum verification."
  }
  Expand-Archive -Path $archivePath -DestinationPath $workDirectory

  $binary = Join-Path $workDirectory "network-cartographer-windows-x86_64/netcart.exe"
  if (-not (Test-Path $binary)) {
    throw "The downloaded release did not contain the expected binary."
  }

  & $binary
} finally {
  if (Test-Path $workDirectory) {
    Remove-Item -Recurse -Force $workDirectory
  }
}
