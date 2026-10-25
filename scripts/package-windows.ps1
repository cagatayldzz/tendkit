[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [ValidateNotNullOrEmpty()]
  [string]$SourcePath,

  [Parameter(Mandatory = $true)]
  [ValidateNotNullOrEmpty()]
  [string]$ArchivePath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

foreach ($name in @('TendKit.exe', 'tendkit-engine.exe', 'README.md')) {
  if (-not (Test-Path -LiteralPath (Join-Path $SourcePath $name) -PathType Leaf)) {
    throw "Missing Windows package file: $name"
  }
}

Compress-Archive -LiteralPath $SourcePath -DestinationPath $ArchivePath -Force

# Verify the ZIP contents before the workflow uploads it.
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [System.IO.Compression.ZipFile]::OpenRead($ArchivePath)
try {
  $entries = @($zip.Entries | ForEach-Object { $_.FullName.Replace('\', '/') })
  foreach ($name in @('TendKit/TendKit.exe', 'TendKit/tendkit-engine.exe', 'TendKit/README.md')) {
    if ($entries -notcontains $name) {
      throw "Missing ZIP entry: $name"
    }
  }
} finally {
  $zip.Dispose()
}
