$ErrorActionPreference='Stop'
$source=Join-Path $PSScriptRoot '../src-tauri/src/services/repair_service.ps1'
Invoke-Expression ([IO.File]::ReadAllText((Resolve-Path $source),[Text.Encoding]::UTF8))
$rustSource=[IO.File]::ReadAllText((Join-Path $PSScriptRoot '../src-tauri/src/services/repair.rs'),[Text.Encoding]::UTF8)
$restartSource=$rustSource.Substring($rustSource.IndexOf('pub fn restart('))
$template=[regex]::Match($restartSource,'r#"(.*?)"#',[Text.RegularExpressions.RegexOptions]::Singleline).Groups[1].Value
$template=$template.Replace('{{','{').Replace('}}','}')
$template=[regex]::Replace($template,'\{[a-z_]+\}',"'C:\Example folder\value'")
$parseErrors=$null;$parseTokens=$null
[Management.Automation.Language.Parser]::ParseInput($template,[ref]$parseTokens,[ref]$parseErrors)|Out-Null
if($parseErrors.Count){throw ($parseErrors|Out-String)}
$exe='C:\Zapret\bin\winws.exe'
$old='"'+$exe+'" --wf-tcp=443 --hostlist=old.txt'
$new='"'+$exe+'" --wf-tcp=443 --hostlist=riot.txt'
$backup=[IO.Path]::GetTempFileName()
$cancel=[IO.Path]::GetTempFileName()
function Assert($Condition,$Message) { if(-not $Condition){throw $Message} }
function Reset-Services($State='Running') {
 $script:services=@{
  zapret=@{Name='zapret';PathName=$old;State=$State;ProcessId=$(if($State -eq 'Running'){10}else{0});StartMode='Auto'}
  other=@{Name='other';PathName='"C:\Other\bin\winws.exe" --wf-tcp=443';State='Running';ProcessId=20;StartMode='Manual'}
  driver=@{Name='WinDivert';PathName='C:\Windows\System32\drivers\WinDivert.sys';State='Running';ProcessId=0;StartMode='System'}
 }
 $script:mutations=@();$script:failStart=$false;$script:concurrent=$false;$script:RustLMCancelPath=$null
}
function Get-CimInstance($ClassName) {
 Assert ($ClassName -eq 'Win32_Service') 'Unexpected real CIM query'
 foreach($item in $script:services.Values){[pscustomobject]$item.Clone()}
}
function Invoke-CimMethod($InputObject,$MethodName,$Arguments) {
 Assert ($InputObject.Name -eq 'zapret') 'Foreign service mutated'
 $script:mutations+= $MethodName
 $item=$script:services[$InputObject.Name]
 switch($MethodName){
  'Change' { Assert ($Arguments.Count -eq 1 -and $Arguments.ContainsKey('PathName')) 'Only PathName may change';$item.PathName=$Arguments.PathName }
  'StopService' { $item.State='Stopped';$item.ProcessId=0;if($script:concurrent){$item.PathName='"C:\Foreign\winws.exe"';$script:concurrent=$false} }
  'StartService' { if($script:failStart){$script:failStart=$false;return [pscustomobject]@{ReturnValue=2}};$item.State='Running';$item.ProcessId=25 }
  default { throw 'Unexpected service method' }
 }
 [pscustomobject]@{ReturnValue=0}
}
function Start-Sleep($Milliseconds) {}
try {
 Reset-Services
 $selected=Get-RustLMWinwsService $exe @(10)
 Assert ($selected.Name -eq 'zapret') 'Selected installation not found'
 Restart-RustLMWinwsService $selected $new $backup | Out-Null
 Assert ($script:services.zapret.PathName -eq $new -and $script:services.zapret.State -eq 'Running') 'Restart did not apply requested configuration'
 Assert ($script:services.zapret.StartMode -eq 'Auto') 'Startup mode changed'
 $saved=Get-Content -Raw $backup | ConvertFrom-Json
 Assert ($saved.path -eq $old -and $saved.state -eq 'Running') 'Original configuration was not saved'
 Reset-Services 'Stopped'
 Restart-RustLMWinwsService (Get-RustLMWinwsService $exe @()) $new $backup | Out-Null
 Assert ($script:services.zapret.State -eq 'Running') 'Stopped service was not recognized'
 foreach($state in @('Running','Stopped')) {
  Reset-Services $state;$script:failStart=$true
  try {Restart-RustLMWinwsService (Get-RustLMWinwsService $exe @(10)) $new $backup;throw 'Expected failure'} catch {Assert ($_.Exception.Message -match 'SERVICE_ORIGINAL_RESTORED') 'Failed startup did not report restoration'}
  Assert ($script:services.zapret.PathName -eq $old -and $script:services.zapret.State -eq $state) 'Original state was not restored'
 }
 Reset-Services;$script:RustLMCancelPath=$cancel
 try {Restart-RustLMWinwsService (Get-RustLMWinwsService $exe @(10)) $new $backup;throw 'Expected cancellation'} catch {Assert ($_.Exception.Message -match 'OPERATION_CANCELLED.*SERVICE_ORIGINAL_RESTORED') 'Cancellation did not restore original service'}
 Assert ($script:services.zapret.PathName -eq $old -and $script:services.zapret.State -eq 'Running') 'Cancellation changed configuration'
 Reset-Services;$script:concurrent=$true
 try {Restart-RustLMWinwsService (Get-RustLMWinwsService $exe @(10)) $new $backup;throw 'Expected conflict'} catch {Assert ($_.Exception.Message -match 'SERVICE_ROLLBACK_FAILED') 'Concurrent configuration conflict was hidden'}
 Assert ($script:services.zapret.PathName -eq '"C:\Foreign\winws.exe"') 'Concurrent administrator changes overwritten'
 Assert (-not ($script:mutations -contains 'Change')) 'Changed conflicting service'
 Reset-Services;$script:services.duplicate=$script:services.zapret.Clone();$script:services.duplicate.Name='duplicate'
 try {Get-RustLMWinwsService $exe @(10);throw 'Expected ambiguity'} catch {Assert ($_.Exception.Message -match 'SERVICE_AMBIGUOUS') 'Ambiguous service accepted'}
 Assert ($script:mutations.Count -eq 0) 'Ambiguous services mutated'
 '{"service_scenarios":7,"foreign_services_mutated":0,"real_service_commands":0,"restart_template_parsed":true}'
} finally {Remove-Item -LiteralPath $backup,$cancel -ErrorAction SilentlyContinue}
