param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$Destination
)

$ErrorActionPreference = "Stop"
$packageUrl = "https://repo.msys2.org/mingw/ucrt64/mingw-w64-ucrt-x86_64-cbindgen-0.29.4-1-any.pkg.tar.zst"
$packageSha256 = "64069c8e30f35dabc3eadf1643dcfced3779711d4d3d865fb39c4190a2e4a96d"
$executableSha256 = "b25d4385c002c428ed63b87e84ac8a479ac863c8125730073303b9c50086b1e5"

$toolRoot = [System.IO.Path]::GetFullPath($Destination)
New-Item -ItemType Directory -Force -Path $toolRoot | Out-Null
$packagePath = Join-Path $toolRoot "cbindgen-0.29.4-1.pkg.tar.zst"
Invoke-WebRequest -UseBasicParsing -Uri $packageUrl -OutFile $packagePath

$actualPackageHash = (Get-FileHash $packagePath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualPackageHash -ne $packageSha256) {
    throw "cbindgen package hash mismatch: expected $packageSha256, got $actualPackageHash"
}

& tar -xf $packagePath -C $toolRoot
if ($LASTEXITCODE -ne 0) {
    throw "failed to extract the reviewed cbindgen package"
}
$executable = Join-Path $toolRoot "ucrt64\bin\cbindgen.exe"
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
    throw "reviewed cbindgen executable is absent after extraction"
}
$actualExecutableHash = (Get-FileHash $executable -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualExecutableHash -ne $executableSha256) {
    throw "cbindgen executable hash mismatch: expected $executableSha256, got $actualExecutableHash"
}
$version = (& $executable --version).Trim()
if ($version -ne "cbindgen 0.29.4") {
    throw "cbindgen version mismatch: $version"
}

Write-Output $executable
