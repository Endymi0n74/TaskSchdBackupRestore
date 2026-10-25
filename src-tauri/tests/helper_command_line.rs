//! Vérifie que la ligne de commande du processus élevé traverse le **vrai**
//! découpage d'arguments de Windows, sur un processus enfant réel : le binaire
//! est lancé en `--helper-import` **sans** élévation, échoue proprement sur
//! l'archive vide passée en `--dir` (aucun `RegisterTask` atteint), et son
//! fichier de résultat doit arriver à l'emplacement exact demandé, le fichier
//! de réponses étant consommé — malgré espaces et antislash final.

#![cfg(windows)]

use std::fs;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

use taskschdbackuprestore_gui::helper::elevated_params;

#[test]
fn le_binaire_enfant_recoit_les_arguments_intacts() {
    let tmp = tempfile::tempdir().expect("dossier temporaire");

    // Archive **vide** (aucun manifest.json) : `load_and_verify` échoue avant
    // toute écriture de tâche planifiée.
    let dir = tmp.path().join("dossier export");
    fs::create_dir_all(&dir).unwrap();
    let dir_arg = format!("{}\\", dir.display()); // antislash final : le piège

    let work = tmp.path().join("travail");
    fs::create_dir_all(&work).unwrap();
    let answers = work.join("answers-1-1.json");
    fs::write(&answers, b"{}").unwrap();

    let result_dir = tmp.path().join("resultats dir");
    fs::create_dir_all(&result_dir).unwrap();
    let result = result_dir.join("result-1-1.json");

    // Même construction que `run_elevated_import` : la ligne vient du code de
    // production, elle n'est pas réécrite dans le test.
    let params = elevated_params(Path::new(&dir_arg), &answers, &result, None);

    let status = Command::new(env!("CARGO_BIN_EXE_TaskSchdBackupRestoreGui"))
        .raw_arg(&params)
        .status()
        .expect("lancement du binaire");

    assert_eq!(
        status.code(),
        Some(2),
        "une archive vide doit échouer proprement (code 2)"
    );
    assert!(
        result.is_file(),
        "le fichier de résultat doit arriver à l'emplacement exact demandé : {}",
        result.display()
    );
    assert!(
        !answers.exists(),
        "le fichier de réponses doit avoir été consommé : `--answers` a bien été relu"
    );
}
