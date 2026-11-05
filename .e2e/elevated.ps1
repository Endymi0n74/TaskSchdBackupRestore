# Enveloppe elevee : rejoue .e2e/e2e.sh (Git Bash) avec un jeton administrateur.
# Le script bash cree les taches de test, importe en \Archive, verifie puis nettoie.
$ErrorActionPreference = 'Continue'
$root = 'D:\Codex\TaskSchdBackupRestore'
$bash = 'C:\Program Files\Git\bin\bash.exe'
$log  = Join-Path $root '.e2e\e2e-elevated.log'
$exit = Join-Path $root '.e2e\e2e-elevated.exit'

Remove-Item $log, $exit -Force -ErrorAction SilentlyContinue
$who = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
"ADMIN=" + $who.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator) | Add-Content $log

$p = Start-Process -FilePath $bash `
    -ArgumentList '-lc', 'cd /d/Codex/TaskSchdBackupRestore && bash .e2e/e2e.sh >> .e2e/e2e-elevated.log 2>&1' `
    -Wait -PassThru -WindowStyle Hidden
"EXIT=" + $p.ExitCode | Add-Content $exit
