# MARZ VPN: last-resort removal of the Windows kill switch filters, when the app and marzvpn-helper.exe
# --recover are both unusable. Touches only filters named "MARZ VPN kill switch",
# the helper's saved state, and that service. It does not change Defender, Windows Firewall or BFE.
#
# Run in an Administrator PowerShell:
#   powershell -NoProfile -ExecutionPolicy Bypass -File .\marzvpn-wfp-recover.ps1          (only lists)
#   powershell -NoProfile -ExecutionPolicy Bypass -File .\marzvpn-wfp-recover.ps1 -Apply   (removes)
param([switch]$Apply)
$ErrorActionPreference = 'Stop'
# The helper names every filter it adds this way (crates/marz-helper/src/wfp.rs).
$namePattern = 'MARZ VPN kill switch:*'

$me = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $me.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Run this in an Administrator PowerShell.' }

function Get-MarzFilterIds {
    $file = Join-Path $env:TEMP "marz-wfp-$PID.xml"
    & netsh wfp show filters file=$file | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'netsh wfp show filters failed.' }
    [xml]$xml = Get-Content -Raw -Path $file
    Remove-Item -Force $file
    @($xml.SelectNodes('//filters/item') | Where-Object { $_.displayData.name -like $namePattern } | ForEach-Object { [uint64]$_.filterId })
}

$ids = Get-MarzFilterIds
Write-Host "MARZ VPN filters found: $($ids.Count)"
if (-not $Apply) { Write-Host 'Nothing was changed. Add -Apply to remove them.'; exit 0 }

# The service would put the block back while the saved state says ON: state first, then the service.
$state = Join-Path $env:ProgramData 'MARZ VPN\killswitch.json'
if (Test-Path $state) { Remove-Item -Force $state; Write-Host 'Saved ON state removed.' }
$svc = Get-Service -Name MarzVpnHelper -ErrorAction SilentlyContinue
if ($svc -and $svc.Status -ne 'Stopped') { Stop-Service -Name MarzVpnHelper -Force; Write-Host 'Service stopped.' }
Get-Process -Name marzvpn-engine -ErrorAction SilentlyContinue | Where-Object { $_.Path -like '*\MARZ VPN\marzvpn-engine.exe' } | Stop-Process -Force

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class MarzWfp {
    [DllImport("fwpuclnt.dll")] public static extern uint FwpmEngineOpen0(IntPtr server, uint authn, IntPtr identity, IntPtr session, out IntPtr engine);
    [DllImport("fwpuclnt.dll")] public static extern uint FwpmFilterDeleteById0(IntPtr engine, ulong id);
    [DllImport("fwpuclnt.dll")] public static extern uint FwpmEngineClose0(IntPtr engine);
}
'@
$engine = [IntPtr]::Zero
$rc = [MarzWfp]::FwpmEngineOpen0([IntPtr]::Zero, 10, [IntPtr]::Zero, [IntPtr]::Zero, [ref]$engine)
if ($rc -ne 0) { throw ('FwpmEngineOpen0 failed: 0x{0:x8}' -f $rc) }
$ids = Get-MarzFilterIds
foreach ($id in $ids) {
    $rc = [MarzWfp]::FwpmFilterDeleteById0($engine, $id)
    if ($rc -ne 0) { Write-Warning ('Filter {0}: 0x{1:x8}' -f $id, $rc) }
}
[void][MarzWfp]::FwpmEngineClose0($engine)
$left = (Get-MarzFilterIds).Count
Write-Host "MARZ VPN filters left: $left"
if ($svc) { Start-Service -Name MarzVpnHelper; Write-Host 'Service started (VPN off).' }
if ($left -ne 0) { exit 1 }
Write-Host 'Done. Test: curl.exe -4 https://www.msftconnecttest.com/connecttest.txt'
