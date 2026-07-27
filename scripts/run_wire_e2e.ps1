param(
  [string]$ArtifactDirectory = "artifacts/wire-e2e"
)

if ($PSVersionTable.PSEdition -ne "Core" -or $PSVersionTable.PSVersion.Major -lt 7) {
  throw "Wire E2E validation requires PowerShell Core 7 or newer. Run this script with pwsh."
}

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$artifactRoot = [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot $ArtifactDirectory))
$targetDirectory = [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot "target/debug"))
$executableSuffix = if ($IsWindows) { ".exe" } else { "" }
$runnerExecutable = Join-Path $targetDirectory "nnrp-conformance-runner$executableSuffix"
$targetExecutable = Join-Path $targetDirectory "nnrp-wire-reference-target$executableSuffix"
$hostRouteTargetExecutable = Join-Path $targetDirectory "nnrp-wire-host-route-reference-target$executableSuffix"
$clientOnlyHostRouteTargetExecutable = Join-Path $targetDirectory "nnrp-wire-host-route-client-only-reference-target$executableSuffix"
$serverOnlyHostRouteTargetExecutable = Join-Path $targetDirectory "nnrp-wire-host-route-server-only-reference-target$executableSuffix"

New-Item -ItemType Directory -Force -Path $artifactRoot | Out-Null

$targetManifest = Join-Path $artifactRoot "target.json"
$executionPlan = Join-Path $artifactRoot "plan.json"
$resultReport = Join-Path $artifactRoot "results.json"
$uninstalledTargetManifest = Join-Path $artifactRoot "target-uninstalled-quic.json"
$uninstalledExecutionPlan = Join-Path $artifactRoot "plan-uninstalled-quic.json"
$uninstalledResultReport = Join-Path $artifactRoot "results-uninstalled-quic.json"
$hostRouteOnlyTargetManifest = Join-Path $artifactRoot "target-host-route-only.json"
$hostRouteOnlyExecutionPlan = Join-Path $artifactRoot "plan-host-route-only.json"
$clientOnlyResultReport = Join-Path $artifactRoot "results-client-only.json"
$serverOnlyResultReport = Join-Path $artifactRoot "results-server-only.json"
$clientOnlyErrorLog = Join-Path $artifactRoot "client-only.expected-error.log"
$serverOnlyErrorLog = Join-Path $artifactRoot "server-only.expected-error.log"
$targetStdout = Join-Path $artifactRoot "target.stdout.log"
$targetStderr = Join-Path $artifactRoot "target.stderr.log"
$evidenceDirectory = Join-Path $artifactRoot "evidence"
$uninstalledEvidenceDirectory = Join-Path $artifactRoot "evidence-uninstalled-quic"
$clientOnlyEvidenceDirectory = Join-Path $artifactRoot "evidence-client-only"
$serverOnlyEvidenceDirectory = Join-Path $artifactRoot "evidence-server-only"

foreach ($path in @(
  $targetManifest,
  $executionPlan,
  $resultReport,
  $uninstalledTargetManifest,
  $uninstalledExecutionPlan,
  $uninstalledResultReport,
  $hostRouteOnlyTargetManifest,
  $hostRouteOnlyExecutionPlan,
  $clientOnlyResultReport,
  $serverOnlyResultReport,
  $clientOnlyErrorLog,
  $serverOnlyErrorLog,
  $targetStdout,
  $targetStderr
)) {
  if (Test-Path -LiteralPath $path) {
    Remove-Item -LiteralPath $path -Force
  }
}

foreach ($path in @(
  $evidenceDirectory,
  $uninstalledEvidenceDirectory,
  $clientOnlyEvidenceDirectory,
  $serverOnlyEvidenceDirectory
)) {
  if (Test-Path -LiteralPath $path) {
    Remove-Item -LiteralPath $path -Recurse -Force
  }
}

function Assert-SingularRoleResult {
  param(
    [Parameter(Mandatory = $true)] [string]$PlanPath,
    [Parameter(Mandatory = $true)] [string]$ResultPath,
    [Parameter(Mandatory = $true)] [ValidateSet("client", "server")] [string]$SupportedRole
  )

  $plan = Get-Content -LiteralPath $PlanPath -Raw | ConvertFrom-Json
  $report = Get-Content -LiteralPath $ResultPath -Raw | ConvertFrom-Json
  if ($plan.scenarios.Count -ne 8) {
    throw "$SupportedRole-only target expected the eight installed native host-route scenarios, got $($plan.scenarios.Count)."
  }
  if ($plan.scenarios.Count -ne $report.results.Count) {
    throw "$SupportedRole-only target returned $($report.results.Count) results for $($plan.scenarios.Count) scenarios."
  }

  $passed = 0
  $failed = 0
  foreach ($scenario in $plan.scenarios) {
    $result = @($report.results | Where-Object id -eq $scenario.id)
    if ($result.Count -ne 1) {
      throw "$SupportedRole-only target did not return exactly one result for $($scenario.id)."
    }
    if ($scenario.host_route.role -eq $SupportedRole) {
      if ($result[0].outcome -ne "passed") {
        throw "$SupportedRole-only target failed its supported $($scenario.host_route.role) scenario $($scenario.id)."
      }
      $passed += 1
    }
    else {
      if ($result[0].outcome -ne "failed" -or $result[0].message -notmatch "implements only the $SupportedRole host role") {
        throw "$SupportedRole-only target did not explicitly reject $($scenario.host_route.role) scenario $($scenario.id)."
      }
      $failed += 1
    }
  }
  if ($passed -ne 4 -or $failed -ne 4) {
    throw "$SupportedRole-only target expected four supported and four rejected scenarios, got $passed and $failed."
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
    --host-route-target $hostRouteTargetExecutable `
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

& $targetExecutable `
  --manifest $uninstalledTargetManifest `
  --profile uninstalled-quic
if ($LASTEXITCODE -ne 0) {
  throw "Failed to publish the uninstalled-QUIC target manifest."
}

& $runnerExecutable wire-plan `
  --suite (Join-Path $repositoryRoot "wire-conformance/nnrp-1-preview4/manifest.json") `
  --target $uninstalledTargetManifest `
  --output $uninstalledExecutionPlan `
  --results-path $uninstalledResultReport `
  --evidence-dir $uninstalledEvidenceDirectory
if ($LASTEXITCODE -ne 0) {
  throw "uninstalled-QUIC wire-plan failed with exit code $LASTEXITCODE."
}

& $runnerExecutable wire-run `
  --plan $uninstalledExecutionPlan `
  --target $uninstalledTargetManifest `
  --host-route-target $hostRouteTargetExecutable `
  --output $uninstalledResultReport
if ($LASTEXITCODE -ne 0) {
  throw "uninstalled-QUIC wire-run failed with exit code $LASTEXITCODE."
}

& $runnerExecutable validate-wire-results `
  --plan $uninstalledExecutionPlan `
  --results $uninstalledResultReport
if ($LASTEXITCODE -ne 0) {
  throw "uninstalled-QUIC validate-wire-results failed with exit code $LASTEXITCODE."
}

& $targetExecutable `
  --manifest $hostRouteOnlyTargetManifest `
  --profile host-route-only
if ($LASTEXITCODE -ne 0) {
  throw "Failed to publish the host-route-only target manifest."
}

& $runnerExecutable wire-plan `
  --suite (Join-Path $repositoryRoot "wire-conformance/nnrp-1-preview4/manifest.json") `
  --target $hostRouteOnlyTargetManifest `
  --output $hostRouteOnlyExecutionPlan `
  --results-path $clientOnlyResultReport `
  --evidence-dir $clientOnlyEvidenceDirectory
if ($LASTEXITCODE -ne 0) {
  throw "host-route-only wire-plan failed with exit code $LASTEXITCODE."
}

foreach ($singularTarget in @(
  @{
    Role = "client"
    Executable = $clientOnlyHostRouteTargetExecutable
    Results = $clientOnlyResultReport
    Evidence = $clientOnlyEvidenceDirectory
    ErrorLog = $clientOnlyErrorLog
  },
  @{
    Role = "server"
    Executable = $serverOnlyHostRouteTargetExecutable
    Results = $serverOnlyResultReport
    Evidence = $serverOnlyEvidenceDirectory
    ErrorLog = $serverOnlyErrorLog
  }
)) {
  $plan = Get-Content -LiteralPath $hostRouteOnlyExecutionPlan -Raw | ConvertFrom-Json
  $plan.artifacts.results_path = $singularTarget.Results
  $plan.artifacts.evidence_dir = $singularTarget.Evidence
  $plan | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath $hostRouteOnlyExecutionPlan

  & $runnerExecutable wire-run `
    --plan $hostRouteOnlyExecutionPlan `
    --target $hostRouteOnlyTargetManifest `
    --host-route-target $singularTarget.Executable `
    --output $singularTarget.Results `
    2> $singularTarget.ErrorLog
  if ($LASTEXITCODE -eq 0) {
    throw "$($singularTarget.Role)-only host-route target unexpectedly passed every scenario."
  }
  Assert-SingularRoleResult `
    -PlanPath $hostRouteOnlyExecutionPlan `
    -ResultPath $singularTarget.Results `
    -SupportedRole $singularTarget.Role

  & $runnerExecutable validate-wire-results `
    --plan $hostRouteOnlyExecutionPlan `
    --results $singularTarget.Results
  if ($LASTEXITCODE -ne 0) {
    throw "$($singularTarget.Role)-only target did not produce a complete, valid failure report."
  }
}

Get-Content -LiteralPath $resultReport
Get-Content -LiteralPath $uninstalledResultReport
