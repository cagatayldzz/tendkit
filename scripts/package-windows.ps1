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

foreach ($name in @('TendKit.exe')) {
  if (-not (Test-Path -LiteralPath (Join-Path $SourcePath $name) -PathType Leaf)) {
    throw "Missing Windows package file: $name"
  }
}

Compress-Archive -LiteralPath (Join-Path $SourcePath 'TendKit.exe') -DestinationPath $ArchivePath -Force

# Verify the ZIP contents before the workflow uploads it.
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [System.IO.Compression.ZipFile]::OpenRead($ArchivePath)
try {
  $entries = @($zip.Entries | ForEach-Object { $_.FullName.Replace('\', '/') })
  if ($entries.Count -ne 1) {
    throw 'Windows portable ZIP must contain exactly one file'
  }
  foreach ($name in @('TendKit.exe')) {
    if ($entries -notcontains $name) {
      throw "Missing ZIP entry: $name"
    }
  }
} finally {
  $zip.Dispose()
}
