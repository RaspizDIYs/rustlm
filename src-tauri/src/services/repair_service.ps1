# Only the selected installation is managed; driver services are never selected.
function Get-RustLMWinwsService($Executable, $ProcessIds) {
    $pattern = '^(?:"' + [regex]::Escape($Executable) + '"|' + [regex]::Escape($Executable) + '(?=\s|$))'
    $services = @(Get-CimInstance Win32_Service | Where-Object {
        $byPid = $_.ProcessId -ne 0 -and $ProcessIds -contains $_.ProcessId
        $byPath = $_.PathName -match $pattern
        if ($byPid -and -not $byPath) { throw 'SERVICE_SCOPE_CHANGED: конфигурация службы изменилась' }
        $byPath
    })
    if ($services.Count -gt 1) { throw 'SERVICE_AMBIGUOUS: найдено несколько служб выбранной установки' }
    if ($services.Count) { return $services[0] }
}
function Read-RustLMService($Name) {
    $found = @(Get-CimInstance Win32_Service | Where-Object { $_.Name -eq $Name })
    if ($found.Count -ne 1) { throw 'SERVICE_SCOPE_CHANGED: выбранная служба больше не найдена' }
    return $found[0]
}
function Set-RustLMServiceState($Name, $State) {
    $service = Read-RustLMService $Name
    if ($service.State -eq $State) { return $service }
    $method = if ($State -eq 'Stopped') { 'StopService' } else { 'StartService' }
    $result = Invoke-CimMethod -InputObject $service -MethodName $method
    $until = [DateTime]::UtcNow.AddSeconds(12)
    do {
        $service = Read-RustLMService $Name
        if ($service.State -eq $State) { return $service }
        if ($result.ReturnValue -ne 0) { throw ('SERVICE_CONTROL_FAILED: код Windows ' + $result.ReturnValue) }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $until)
    throw ('SERVICE_CONTROL_TIMEOUT: служба не перешла в состояние ' + $State)
}
function Set-RustLMServicePath($Name, $Path) {
    $result = Invoke-CimMethod -InputObject (Read-RustLMService $Name) -MethodName Change -Arguments @{PathName=$Path}
    if ($result.ReturnValue -ne 0) { throw ('SERVICE_CHANGE_FAILED: код Windows ' + $result.ReturnValue) }
}
function Restore-RustLMService($Name, $OriginalPath, $OriginalState, $NewPath) {
    $current = Read-RustLMService $Name
    if ($current.PathName -ne $NewPath -and $current.PathName -ne $OriginalPath) { throw 'SERVICE_ROLLBACK_CONFLICT: параметры изменены другим процессом' }
    if ($current.PathName -eq $OriginalPath -and $current.State -eq $OriginalState) { return }
    Set-RustLMServiceState $Name 'Stopped' | Out-Null
    $current = Read-RustLMService $Name
    if ($current.PathName -ne $NewPath -and $current.PathName -ne $OriginalPath) { throw 'SERVICE_ROLLBACK_CONFLICT: параметры изменены во время остановки службы' }
    Set-RustLMServicePath $Name $OriginalPath
    Set-RustLMServiceState $Name $OriginalState | Out-Null
}
function Restart-RustLMWinwsService($Service, $NewPath, $Backup) {
    $name = $Service.Name
    $originalPath = $Service.PathName
    $originalState = $Service.State
    if ($originalState -notin @('Running','Stopped')) { throw 'SERVICE_BUSY: дождитесь завершения другой операции службы' }
    @{name=$name;path=$originalPath;state=$originalState} | ConvertTo-Json -Compress | Set-Content -LiteralPath $Backup -Encoding UTF8
    if ((Read-RustLMService $name).PathName -ne $originalPath) { throw 'SERVICE_SCOPE_CHANGED: служба изменена другим процессом' }
    try {
        $script:RustLMServiceTouched=$true
        Set-RustLMServiceState $name 'Stopped' | Out-Null
        if ($script:RustLMCancelPath -and (Test-Path -LiteralPath $script:RustLMCancelPath)) { throw 'OPERATION_CANCELLED' }
        if ((Read-RustLMService $name).PathName -ne $originalPath) { throw 'SERVICE_SCOPE_CHANGED: служба изменена другим процессом' }
        Set-RustLMServicePath $name $NewPath
        if ((Read-RustLMService $name).PathName -ne $NewPath) { throw 'SERVICE_SCOPE_CHANGED: параметры изменены другим процессом' }
        $running = Set-RustLMServiceState $name 'Running'
        Start-Sleep -Milliseconds 500
        $running = Read-RustLMService $name
        if ($running.State -ne 'Running' -or $running.ProcessId -eq 0) { throw 'SERVICE_CONTROL_FAILED: служба завершилась после запуска' }
        return $running
    } catch {
        $failure = $_.Exception.Message
        try {
            Restore-RustLMService $name $originalPath $originalState $NewPath
            $script:RustLMServiceTouched=$false
        } catch { throw ($failure + '; SERVICE_ROLLBACK_FAILED: ' + $_.Exception.Message) }
        throw ($failure + '; SERVICE_ORIGINAL_RESTORED')
    }
}
