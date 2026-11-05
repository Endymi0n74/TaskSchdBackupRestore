set -u
cd /d/Codex/TaskSchdBackupRestore
export MSYS2_ARG_CONV_EXCL='*'
W=.e2e/run
rm -rf "$W"; mkdir -p "$W"
CLI=./dist/TaskSchdBackupRestore.exe
GUI=./dist/TaskSchdBackupRestoreGui.exe
EXPORT="$W/export-source"

pass() { echo "OK    $1"; }
fail() { echo "ECHEC $1"; }
check() { if [ "$2" = "0" ]; then pass "$1"; else fail "$1"; fi; }

cleanup_tasks() {
  schtasks /delete /tn '\BackupTasksFolder\ZTestMsr' /f >/dev/null 2>&1
  schtasks /delete /tn '\BackupTasksFolder\ZTestWin' /f >/dev/null 2>&1
  schtasks /delete /tn '\BackupTasksFolder' /f >/dev/null 2>&1
  schtasks /delete /tn '\ZTestSource\ZTestMsr' /f >/dev/null 2>&1
  schtasks /delete /tn '\ZTestSource' /f >/dev/null 2>&1
}
cleanup_tasks

echo "=== 1. tache source reelle (XML valide) : \\ZTestSource\\ZTestMsr ==="
schtasks /create /tn '\ZTestSource\ZTestMsr' /tr "cmd /c exit 0" /sc once /st 23:55 /f 2>&1 | sed 's/^/  /'
echo "  create exit=${PIPESTATUS[0]}"

echo "=== 2. export par le binaire dist ==="
"$CLI" export "$EXPORT" --include '\ZTestSource\ZTest*' 2>&1 | sed 's/^/  /'
echo "  export exit=${PIPESTATUS[0]}"

echo "=== 3. archive de test : \\Microsoft\\ZTestMsr, \\Microsoft\\Windows\\ZTestWin, \\ZTestCollision\\ZTestMsr ==="
node .e2e/prepare.js | sed 's/^/  /'
sed 's/^/  /' "$EXPORT/manifest.json"

echo "=== 4. validate ==="
"$CLI" validate "$EXPORT" 2>&1 | sed 's/^/  /'
VAL=$?
check "archive valide" "$VAL"

UID1=$(node -e 'console.log(require("./.e2e/run/export-source/manifest.json").tasks[0].user_id||"")')

echo "=== 5. plan dry-run --folder \\BackupTasksFolder (binaire CLI de dist) ==="
"$CLI" import "$EXPORT" --folder '\BackupTasksFolder' --dry-run --yes --skip-password-tasks --user-map "$UID1:$UID1" >"$W/dry.txt" 2>&1
echo "  dry-run exit=$?"
sed 's/^/  /' "$W/dry.txt"

grep -qF 'CREATE' "$W/dry.txt"; check "une creation est planifiee" $?
grep -qF '\BackupTasksFolder\ZTestMsr' "$W/dry.txt"; check "chemin cible \\BackupTasksFolder\\ZTestMsr (source \\Microsoft\\ZTestMsr)" $?
grep -qF '\BackupTasksFolder\ZTestWin' "$W/dry.txt"; check "chemin cible \\BackupTasksFolder\\ZTestWin (source \\Microsoft\\Windows\\ZTestWin)" $?
grep -qF 'CIBLE EN DOUBLE' "$W/dry.txt"; check "CIBLE EN DOUBLE pour \\ZTestCollision\\ZTestMsr (collision)" $?
! grep -qF '\BackupTasksFolder\Microsoft' "$W/dry.txt"; check "aucun \\BackupTasksFolder\\Microsoft propose" $?
! grep -qF '\BackupTasksFolder\Windows' "$W/dry.txt"; check "aucun \\BackupTasksFolder\\Windows propose" $?
! grep -qF '\BackupTasksFolder\ZTestCollision' "$W/dry.txt"; check "aucun \\BackupTasksFolder\\ZTestCollision propose" $?

echo "=== 6. import reel via le binaire GUI (mode helper) ==="
"$GUI" --helper-import --dir "$EXPORT" --answers "$W/answers.json" --result "$W/result.json" --folder '\BackupTasksFolder' >"$W/gui.txt" 2>&1
echo "  helper exit=$?"
sed 's/^/  /' "$W/gui.txt"
if [ -f "$W/result.json" ]; then
  echo "  --- result.json ---"
  sed 's/^/  /' "$W/result.json"
  grep -qF '"ok": true' "$W/result.json"; check "helper GUI : resultat ok" $?
  grep -qF '\BackupTasksFolder\ZTestMsr' "$W/result.json"; check "helper GUI : \\BackupTasksFolder\\ZTestMsr" $?
  grep -qF '\BackupTasksFolder\ZTestWin' "$W/result.json"; check "helper GUI : \\BackupTasksFolder\\ZTestWin" $?
  ! grep -qF 'Archive\\\\Microsoft' "$W/result.json"; check "helper GUI : aucun \\BackupTasksFolder\\Microsoft dans le rapport" $?
else
  fail "helper GUI : result.json absent (jeton non eleve ?)"
fi

echo "=== 7. etat reel du planificateur ==="
schtasks /query /fo csv /nh >"$W/all.csv" 2>&1
grep -iF '\BackupTasksFolder\' "$W/all.csv" | cut -d, -f1 | sed 's/^/  /'
grep -qiF '\BackupTasksFolder\ZTestMsr' "$W/all.csv"; check "\\BackupTasksFolder\\ZTestMsr existe dans le planificateur" $?
grep -qiF '\BackupTasksFolder\ZTestWin' "$W/all.csv"; check "\\BackupTasksFolder\\ZTestWin existe dans le planificateur" $?
! grep -qiF '\BackupTasksFolder\Microsoft' "$W/all.csv"; check "aucune tache \\BackupTasksFolder\\Microsoft\\*" $?
! grep -qiF '\BackupTasksFolder\Windows' "$W/all.csv"; check "aucune tache \\BackupTasksFolder\\Windows\\*" $?
NB=$(grep -icF '\BackupTasksFolder\' "$W/all.csv")
[ "$NB" = "2" ]; check "exactement 2 taches sous \\BackupTasksFolder (trouve : $NB)" $?

echo "=== 8. second import (CLI) : rien a refaire ==="
"$CLI" import "$EXPORT" --folder '\BackupTasksFolder' --yes --skip-password-tasks --user-map "$UID1:$UID1" >"$W/again.txt" 2>&1
echo "  exit=$?"
sed 's/^/  /' "$W/again.txt"

echo "=== 9. nettoyage ==="
cleanup_tasks
REST=$(schtasks /query /fo csv /nh 2>&1 | grep -c 'ZTest')
[ "$REST" = "0" ]; check "aucune tache de test restante (reste : $REST)" $?
echo "=== FIN ==="
