@echo off
cd /d "%~dp0"
echo ============================================================
echo  VALIDATION de l'archive
echo ============================================================
"..\..\TaskSchdBackupRestore.exe" validate ".\export-demo"
echo.
echo ============================================================
echo  IMPORT (simulation --dry-run) avec dossier cible
echo ============================================================
"..\..\TaskSchdBackupRestore.exe" import ".\export-demo" --folder "\Restauration-2026-09-09" --dry-run --yes --skip-password-tasks
echo.
echo Termine - appuyez sur une touche pour fermer
pause >nul