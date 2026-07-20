param(
  [string]$ArtifactDirectory = "artifacts/wire-e2e"
)

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$artifactRoot = [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot $ArtifactDirectory))
$targetDirectory = [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot "target/debug"))
$executableSuffix = if ($IsWindows) { ".exe" } else { "" }
$runnerExecutable = Join-Path $targetDirectory "nnrp-conformance-runner$executableSuffix"
$targetExecutable = Join-Path $targetDirectory "nnrp-wire-reference-target$executableSuffix"

New-Item -ItemType Directory -Force -Path $artifactRoot | Out-Null

$targetManifest = Join-Path $artifactRoot "target.json"
$executionPlan = Join-Path $artifactRoot "plan.json"
$resultReport = Join-Path $artifactRoot "results.json"
$targetStdout = Join-Path $artifactRoot "target.stdout.log"
$targetStderr = Join-Path $artifactRoot "target.stderr.log"
$evidenceDirectory = Join-Path $artifactRoot "evidence"

foreach ($path in @($targetManifest, $executionPlan, $resultReport, $targetStdout, $targetStderr)) {
  if (Test-Path -LiteralPath $path) {
    Remove-Item -LiteralPath $path -Force
  }
}

cargo build -p nnrp-conformance-runner --bins
if ($LASTEXITCODE -ne 0) {
  throw "Failed to build the wire-conformance runner binaries."
}

$startInfo = [System.Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = $targetExecutable
$startInfo.ArgumentList.Add("--manifest")
$startInfo.ArgumentList.Add($targetManifest)
$startInfo.WorkingDirectory = $repositoryRoot
$startInfo.UseShellExecute = $false
$startInfo.RedirectStandardOutput = $true
$startInfo.RedirectStandardError = $true

$targetProcess = [System.Diagnostics.Process]::new()
$targetProcess.StartInfo = $startInfo
if (-not $targetProcess.Start()) {
  throw "Failed to start the independent wire-conformance target process."
}

try {
  $ready = $false
  for ($attempt = 0; $attempt -lt 100; $attempt += 1) {
    if (Test-Path -LiteralPath $targetManifest) {
      $ready = $true
      break
    }
    if ($targetProcess.HasExited) {
      $stderr = $targetProcess.StandardError.ReadToEnd()
      throw "Wire target exited before publishing its manifest (exit $($targetProcess.ExitCode)): $stderr"
    }
    Start-Sleep -Milliseconds 100
  }
  if (-not $ready) {
    throw "Wire target did not publish its manifest within 10 seconds."
  }

  & $runnerExecutable wire-plan `
    --suite (Join-Path $repositoryRoot "wire-conformance/nnrp-1-preview4/manifest.json") `
    --target $targetManifest `
    --output $executionPlan `
    --results-path $resultReport `
    --evidence-dir $evidenceDirectory
  if ($LASTEXITCODE -ne 0) {
    throw "wire-plan failed with exit code $LASTEXITCODE."
  }

  & $runnerExecutable wire-run `
    --plan $executionPlan `
    --target $targetManifest `
    --output $resultReport
  if ($LASTEXITCODE -ne 0) {
    throw "wire-run failed with exit code $LASTEXITCODE."
  }

  & $runnerExecutable validate-wire-results `
    --plan $executionPlan `
    --results $resultReport
  if ($LASTEXITCODE -ne 0) {
    throw "validate-wire-results failed with exit code $LASTEXITCODE."
  }

  if (-not $targetProcess.WaitForExit(10000)) {
    throw "Wire target did not finish after the suite completed all scenarios."
  }
  $targetProcess.StandardOutput.ReadToEnd() | Set-Content -LiteralPath $targetStdout
  $targetProcess.StandardError.ReadToEnd() | Set-Content -LiteralPath $targetStderr
  if ($targetProcess.ExitCode -ne 0) {
    throw "Wire target failed with exit code $($targetProcess.ExitCode). See $targetStderr."
  }
} finally {
  if (-not $targetProcess.HasExited) {
    $targetProcess.Kill($true)
    $targetProcess.WaitForExit()
  }
  $targetProcess.Dispose()
}

Get-Content -LiteralPath $resultReport
