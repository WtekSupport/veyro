# Adds Rust/Cargo to PATH when rustup shims are missing from the shell.

function Merge-UniquePath {
    param([string[]]$Segments)

    $seen = [System.Collections.Generic.HashSet[string]]::new(
        [StringComparer]::OrdinalIgnoreCase
    )
    $merged = [System.Collections.Generic.List[string]]::new()

    foreach ($segment in $Segments) {
        if ([string]::IsNullOrWhiteSpace($segment)) {
            continue
        }

        foreach ($part in ($segment -split ';')) {
            $part = $part.Trim()
            if ($part -and $seen.Add($part)) {
                [void]$merged.Add($part)
            }
        }
    }

    return ($merged -join ';')
}

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$machinePath = [Environment]::GetEnvironmentVariable("Path", "Machine")
$cargoShim = Join-Path $env:USERPROFILE ".cargo\bin"

$prepend = @()
if (Test-Path $cargoShim) {
    $prepend += $cargoShim
}

$toolchainsRoot = Join-Path $env:USERPROFILE ".rustup\toolchains"
if (Test-Path $toolchainsRoot) {
    $stableBin = Join-Path $toolchainsRoot "stable-x86_64-pc-windows-msvc\bin"
    if (Test-Path (Join-Path $stableBin "cargo.exe")) {
        $prepend += $stableBin
    } else {
        $fallbackBin = Get-ChildItem $toolchainsRoot -Directory -ErrorAction SilentlyContinue |
            ForEach-Object { Join-Path $_.FullName "bin" } |
            Where-Object { Test-Path (Join-Path $_ "cargo.exe") } |
            Select-Object -First 1
        if ($fallbackBin) {
            $prepend += $fallbackBin
        }
    }
}

$env:PATH = Merge-UniquePath @($prepend + $env:PATH + $userPath + $machinePath)
