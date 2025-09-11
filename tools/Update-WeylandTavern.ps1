param(
  # Accepts: origin/<branch>, <branch>, tags/<tag>, or a full/short <SHA>
  [string]$Ref = "origin/nightly",
  # If set, check out the exact remote ref in detached HEAD (no local branch)
  [switch]$PinExact
)

# Fail fast on errors inside PowerShell
$ErrorActionPreference = "Stop"
if ($PSVersionTable.PSVersion.Major -ge 7) {
  # In PS7+, keep native command error behavior consistent with $ErrorActionPreference
  $global:PSNativeCommandUseErrorActionPreference = $false
}

function Invoke-Git {
  param(
    [Parameter(Mandatory=$true)][string[]]$Args,
    [switch]$Quiet
  )
  # Run git, redirect stderr -> stdout so we can display everything on failure
  $out = & git @Args 2>&1
  $code = $LASTEXITCODE

  # Echo output unless suppressed
  if (-not $Quiet) { $out | Write-Host }

  # Uniform error handling with full command context
  if ($code -ne 0) {
    throw ("git {0} failed (exit {1})`n{2}" -f ($Args -join ' '), $code, ($out -join "`n"))
  }
  return $out
}

# 1) Move to the repository root (script is assumed to live within the repo)
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot   = Resolve-Path (Join-Path $scriptRoot "..")
Set-Location $repoRoot
Write-Host "Working directory set to $repoRoot"

# 2) Ensure submodule exists and is initialized
#    Note: forward slashes are used to keep paths Git-friendly
$sub = "vendor/WeylandTavern"
Write-Host "Ensuring submodule initialized..."
Invoke-Git @("submodule","update","--init","--",$sub)

# 3) Determine type of ref and fetch minimal data required
Push-Location $sub

# Heuristics for ref classification
$refIsSHA  = $Ref -match '^[0-9a-f]{7,40}$'
$refIsTag  = $Ref -like 'tags/*'
$refIsOrig = $Ref -like 'origin/*'
$branchName = $null

if ($refIsSHA) {
  Write-Host "Fetching specific commit $Ref ..."
  Invoke-Git @("fetch","origin",$Ref,"--depth","1")
} elseif ($refIsTag) {
  $tagName = $Ref -replace '^tags/',''
  Write-Host "Fetching tags (will check out tag '$tagName') ..."
  Invoke-Git @("fetch","--tags")
} else {
  # If ref is origin/<branch>, strip the prefix; otherwise treat it as a branch name
  $branchName = ($refIsOrig) ? ($Ref -replace '^origin/','') : $Ref
  Write-Host "Fetching remote branch 'origin/$branchName' ..."
  Invoke-Git @("fetch","origin",$branchName,"--depth","1")
}

# 4) Check out the requested ref
if ($refIsSHA) {
  # Directly check out a commit SHA (detached HEAD)
  Invoke-Git @("checkout",$Ref)
} elseif ($refIsTag) {
  # Check out a lightweight/annotated tag in detached mode
  $tagName = $Ref -replace '^tags/',''
  Invoke-Git @("checkout","--detach","tags/$tagName")
} elseif ($branchName) {
  if ($PinExact) {
    # Use the exact remote commit (detached), do not create a local branch
    Invoke-Git @("checkout","--detach","origin/$branchName")
  } else {
    # Recreate local branch tracking the remote (delete if it already exists)
    & git rev-parse --verify $branchName 1>$null 2>$null
    if ($LASTEXITCODE -eq 0) {
      Invoke-Git @("branch","-D",$branchName) -Quiet
    }
    Invoke-Git @("checkout","-b",$branchName,"origin/$branchName")
  }
} else {
  # Fallback: detach to origin/nightly if parsing didn’t match anything
  Invoke-Git @("checkout","--detach","origin/nightly")
}

function Invoke-GitSingle {
  param(
    [Parameter(ValueFromRemainingArguments=$true)]
    [string[]]$Args
  )
  # Thin wrapper around Invoke-Git to get a trimmed single string output
  $out = Invoke-Git -Quiet -Args $Args
  return ([string]($out -join "`n")).Trim()
}

# Resolve the current commit SHA (preferred method)
$sha = Invoke-GitSingle -Args @("rev-parse","HEAD")

Pop-Location

# 5) In superproject, stage the submodule pointer update and commit if changed
Invoke-Git @("add",$sub) -Quiet

# Check if the staged index includes the submodule path and commit only when necessary
$out = (& git diff --cached --name-only 2>&1) -join "`n"
if ($LASTEXITCODE -eq 0 -and $out -and $out.Trim() -ne "") {
  Invoke-Git @("commit","-m","chore(submodule): bump WeylandTavern to $sha") -Quiet
  Write-Host "Pinned WeylandTavern at $sha"
} else {
  Write-Host "Nothing to commit; submodule already at $sha"
}
