//! TaskSchdBackupRestore : export/import des taches planifiees Windows (Task Scheduler) au
//! format XML brut, pour migration ou restauration entre machines.
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod action;
pub mod answers;
pub mod error;
pub mod export;
pub mod hash;
pub mod import;
pub mod log;
pub mod model;
pub mod password;
pub mod scheduler;
pub mod wizard;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Dossier de donnees de l'application, sous `%LOCALAPPDATA%` : journaux et
/// dossiers d'annulation du CLI comme de l'interface.
pub const APP_DIR: &str = "TaskSchdBackupRestore";

/// Nom porte par ce dossier avant le renommage (voir [`migrate_legacy_data`]).
const LEGACY_APP_DIR: &str = "tsbak";

/// Dossier de donnees : `%LOCALAPPDATA%\TaskSchdBackupRestore`, repli sur le
/// dossier temporaire (plateformes sans `LOCALAPPDATA`).
///
/// Seul proprietaire du chemin : `log::log_dir`, `import::default_undo_dir` et
/// la journalisation de l'interface le derivent d'ici. Le premier appel reprend
/// les donnees de l'ancien dossier (voir [`migrate_legacy_data`]).
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    static REPRISE: OnceLock<()> = OnceLock::new();
    REPRISE.get_or_init(|| migrate_legacy_data(&base));
    base.join(APP_DIR)
}

/// Reprend une seule fois, au premier demarrage de cette version, les journaux
/// et dossiers d'annulation ecrits dans `%LOCALAPPDATA%\tsbak` : sans cela le
/// renommage les rendrait invisibles a l'application.
///
/// Copie seule, rien n'est supprime ni ecrase : un fichier deja present dans le
/// nouveau dossier garde son contenu. Les journaux sont renommes avec le
/// nouveau prefixe, sinon la purge par prefixe les ignorerait. Best-effort :
/// une reprise partielle ne doit pas empecher l'application de demarrer.
fn migrate_legacy_data(base: &Path) {
    let ancien = base.join(LEGACY_APP_DIR);
    let nouveau = base.join(APP_DIR);
    if ancien.is_dir() && !nouveau.exists() {
        let _ = copy_tree(&ancien, &nouveau);
    }
    let logs_anciens = nouveau.join("logs");
    if let Ok(entrees) = std::fs::read_dir(&logs_anciens) {
        for entree in entrees.flatten() {
            let nom = entree.file_name().to_string_lossy().to_string();
            if let Some(reste) = nom.strip_prefix("tsbak-") {
                let cible = logs_anciens.join(format!("{APP_DIR}-{reste}"));
                if !cible.exists() {
                    let _ = std::fs::rename(entree.path(), cible);
                }
            }
        }
    }
}

/// Copie recursive `ancien` vers `nouveau` sans ecraser un fichier existant.
fn copy_tree(ancien: &Path, nouveau: &Path) -> std::io::Result<()> {
    for entree in walkdir::WalkDir::new(ancien).into_iter().filter_map(Result::ok) {
        if !entree.file_type().is_file() {
            continue;
        }
        let Ok(relatif) = entree.path().strip_prefix(ancien) else {
            continue;
        };
        let cible = nouveau.join(relatif);
        if cible.exists() {
            continue;
        }
        if let Some(parent) = cible.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(entree.path(), &cible)?;
    }
    Ok(())
}

/// Nom de la machine hôte (variable `COMPUTERNAME`, repli `HOSTNAME`, puis
/// `unknown-host`) : figure dans le manifeste d'export et les journaux.
/// Partagé par le CLI et l'interface pour des empreintes strictement
/// identiques d'une interface à l'autre.
pub fn local_host_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown-host".to_string())
}

#[cfg(test)]
mod tests_donnees {
    use super::*;
    use std::fs;

    /// Le renommage ne doit pas rendre invisibles les anciens journaux et
    /// dossiers d'annulation : ils sont repris, journaux renommes avec le
    /// nouveau prefixe, et un fichier deja present n'est pas ecrase.
    #[test]
    fn reprise_des_donnees_de_l_ancien_dossier() {
        let base = tempfile::tempdir().unwrap();
        let ancien = base.path().join(LEGACY_APP_DIR);
        fs::create_dir_all(ancien.join("logs")).unwrap();
        fs::create_dir_all(ancien.join("undo").join("20260101-120000")).unwrap();
        fs::write(ancien.join("logs").join("tsbak-2026-01-01.log"), "vieux").unwrap();
        fs::write(
            ancien.join("undo").join("20260101-120000").join("A.xml"),
            "<Task>vieux</Task>",
        )
        .unwrap();

        migrate_legacy_data(base.path());

        let nouveau = base.path().join(APP_DIR);
        assert_eq!(
            fs::read_to_string(nouveau.join("logs").join(format!("{APP_DIR}-2026-01-01.log"))).unwrap(),
            "vieux",
            "le journal repris doit porter le nouveau prefixe"
        );
        assert_eq!(
            fs::read_to_string(nouveau.join("undo").join("20260101-120000").join("A.xml")).unwrap(),
            "<Task>vieux</Task>",
            "le dossier d'annulation doit etre repris tel quel"
        );

        // Une seconde reprise ne reecrit rien : le fichier en place gagne.
        fs::write(nouveau.join("logs").join(format!("{APP_DIR}-2026-01-01.log")), "neuf").unwrap();
        migrate_legacy_data(base.path());
        assert_eq!(
            fs::read_to_string(nouveau.join("logs").join(format!("{APP_DIR}-2026-01-01.log"))).unwrap(),
            "neuf"
        );
    }
}
