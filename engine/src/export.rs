//! Export des taches planifiees : parcours recursif, ecriture du XML brut
//! (jamais modifie) et du manifeste JSON. Aucun mot de passe n'est jamais
//! lu ni ecrit ici : le XML retourne par le planificateur ne contient de
//! toute facon jamais de secret en clair (Windows le retire automatiquement
//! des taches TASK_LOGON_PASSWORD).

use std::fs;
use std::path::Path;

use crate::error::{Result, BackupError};
use crate::hash::sha256_hex;
use crate::model::{Manifest, TaskRecord};
use crate::scheduler::TaskSchedulerApi;

/// Filtre par motif simple (glob "*" uniquement, insensible a la casse) sur
/// le chemin complet de la tache.
pub struct PatternFilter {
    include: Vec<String>,
    exclude: Vec<String>,
}

impl PatternFilter {
    /// Construit un filtre a partir des motifs `--include`/`--exclude`.
    pub fn new(include: Vec<String>, exclude: Vec<String>) -> Self {
        PatternFilter { include, exclude }
    }

    /// Filtre "masquer les taches systeme" : exclut tout ce qui vit sous
    /// `\Microsoft\` (dossier systeme), comme le fait l'interface graphique.
    pub fn hide_microsoft() -> Self {
        PatternFilter {
            include: Vec::new(),
            exclude: vec!["\\Microsoft\\*".to_string()],
        }
    }

    fn matches_one(pattern: &str, text: &str) -> bool {
        let pattern = pattern.to_lowercase();
        let text = text.to_lowercase();
        if !pattern.contains('*') {
            return text == pattern;
        }
        let parts: Vec<&str> = pattern.split('*').collect();
        let mut pos = 0usize;
        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }
            match text[pos..].find(part) {
                Some(found) => {
                    if i == 0 && found != 0 {
                        return false;
                    }
                    pos += found + part.len();
                }
                None => return false,
            }
        }
        if let Some(last) = parts.last() {
            if !last.is_empty() && !text.ends_with(last) {
                return false;
            }
        }
        true
    }

    /// Indique si le chemin de tache donne passe les filtres inclusion/exclusion.
    pub fn allows(&self, path: &str) -> bool {
        if !self.include.is_empty() && !self.include.iter().any(|p| Self::matches_one(p, path)) {
            return false;
        }
        if self.exclude.iter().any(|p| Self::matches_one(p, path)) {
            return false;
        }
        true
    }
}

/// Indique si une tache vit sous le dossier systeme `\Microsoft\`
/// (insensible a la casse). Utilise par `--hide-microsoft` (list/export).
pub fn is_microsoft_task(path: &str) -> bool {
    path.to_lowercase().starts_with("\\microsoft\\")
}

/// Nom de fichier d'une tache, deduit de son chemin complet.
///
/// Le codage est **injectif** (deux chemins distincts ne peuvent pas donner
/// le meme nom) : `\` devient `__` et un souligne litteral devient `_u` —
/// deux codes de deux caracteres differant par le second, donc le decodage
/// est unique (`\A\B` -> `A__B`, `\A_B` -> `A_uB`). Un chemin sans souligne
/// garde exactement le nom des archives deja exportees
/// (`\Backup\Nightly` -> `Backup__Nightly.xml`) ; les anciennes archives
/// restent importables de toute facon, leur `manifest.json` enregistrant le
/// nom reel de chaque fichier.
pub(crate) fn sanitize_filename(task_path: &str) -> String {
    let trimmed = task_path.trim_start_matches('\\');
    let mut name = String::with_capacity(trimmed.len());
    for c in trimmed.chars() {
        match c {
            '\\' => name.push_str("__"),
            '_' => name.push_str("_u"),
            other => name.push(other),
        }
    }
    if name.is_empty() {
        name = "root".to_string();
    }
    name
}

/// Resume d'une operation d'export.
pub struct ExportSummary {
    /// Chemins des taches effectivement exportees.
    pub exported: Vec<String>,
    /// Chemins des taches ignorees par les filtres --include/--exclude.
    pub skipped_by_filter: Vec<String>,
}

/// Exporte toutes les taches (filtrees par `filter`) vers `out_dir` : un
/// fichier XML brut par tache, plus un `manifest.json` recapitulatif.
pub fn export(
    scheduler: &dyn TaskSchedulerApi,
    out_dir: &Path,
    recursive: bool,
    filter: &PatternFilter,
    source_host: &str,
) -> Result<ExportSummary> {
    fs::create_dir_all(out_dir).map_err(|e| BackupError::Io {
        path: out_dir.display().to_string(),
        source: e,
    })?;

    let handles = scheduler.list_tasks(recursive)?;
    let mut records = Vec::new();
    let mut exported = Vec::new();
    let mut skipped_by_filter = Vec::new();

    for handle in handles {
        if !filter.allows(&handle.path) {
            skipped_by_filter.push(handle.path.clone());
            continue;
        }
        let xml = scheduler.get_task_xml(&handle.path)?;
        let auth = scheduler.get_task_auth_info(&handle.path)?;

        let filename = format!("{}.xml", sanitize_filename(&handle.path));
        let file_path = out_dir.join(&filename);
        fs::write(&file_path, &xml).map_err(|e| BackupError::Io {
            path: file_path.display().to_string(),
            source: e,
        })?;

        records.push(TaskRecord {
            path: handle.path.clone(),
            xml_file: filename,
            sha256: sha256_hex(xml.as_bytes()),
            user_id: auth.user_id,
            logon_type: auth.logon_type,
        });
        exported.push(handle.path);
    }

    let manifest = Manifest::new(source_host.to_string(), records);
    let manifest_path = out_dir.join("manifest.json");
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| BackupError::Other(format!("serialisation du manifeste: {e}")))?;
    fs::write(&manifest_path, manifest_json).map_err(|e| BackupError::Io {
        path: manifest_path.display().to_string(),
        source: e,
    })?;

    Ok(ExportSummary {
        exported,
        skipped_by_filter,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LogonType;
    use crate::scheduler::mock::{MockScheduler, MockTask};
    use tempfile::tempdir;

    fn sample_scheduler() -> MockScheduler {
        MockScheduler::new(true)
            .with_task(
                "\\Backup\\Nightly",
                MockTask {
                    xml: "<Task>nightly</Task>".to_string(),
                    user_id: Some("SYSTEM".to_string()),
                    logon_type: LogonType::ServiceAccount,
                },
            )
            .with_task(
                "\\Backup\\Weekly",
                MockTask {
                    xml: "<Task>weekly</Task>".to_string(),
                    user_id: Some("DOMAIN\\alice".to_string()),
                    logon_type: LogonType::Password,
                },
            )
            .with_task(
                "\\Other\\Cleanup",
                MockTask {
                    xml: "<Task>cleanup</Task>".to_string(),
                    user_id: None,
                    logon_type: LogonType::None,
                },
            )
    }

    #[test]
    fn export_writes_raw_xml_and_manifest() {
        let scheduler = sample_scheduler();
        let dir = tempdir().unwrap();
        let filter = PatternFilter::new(vec![], vec![]);
        let summary = export(&scheduler, dir.path(), true, &filter, "TESTHOST").unwrap();
        assert_eq!(summary.exported.len(), 3);

        let manifest_raw = fs::read_to_string(dir.path().join("manifest.json")).unwrap();
        let manifest: Manifest = serde_json::from_str(&manifest_raw).unwrap();
        assert_eq!(manifest.tasks.len(), 3);

        for record in &manifest.tasks {
            let xml_path = dir.path().join(&record.xml_file);
            let xml_on_disk = fs::read_to_string(&xml_path).unwrap();
            // Le XML sur disque doit correspondre exactement a celui du planificateur
            // (aucune transformation) et son empreinte doit correspondre au manifeste.
            assert_eq!(sha256_hex(xml_on_disk.as_bytes()), record.sha256);
            let original = scheduler.get_task_xml(&record.path).unwrap();
            assert_eq!(xml_on_disk, original);
        }
    }

    #[test]
    fn export_never_contains_password_field_or_secret() {
        // Le manifeste mentionne legitimement le mot "Password" comme etiquette
        // du logon_type (LogonType::Password) ; ce test verifie plutot qu'aucun
        // champ "password"/"secret" ne porte de valeur, et que le XML brut
        // (seule autre donnee ecrite sur disque) ne contient pas non plus le
        // secret utilise par les tests ("hunter2", jamais fourni ici).
        let scheduler = sample_scheduler();
        let dir = tempdir().unwrap();
        let filter = PatternFilter::new(vec![], vec![]);
        export(&scheduler, dir.path(), true, &filter, "TESTHOST").unwrap();

        let manifest_raw = fs::read_to_string(dir.path().join("manifest.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&manifest_raw).unwrap();
        assert!(
            !contains_key(&value, "password") && !contains_key(&value, "secret"),
            "le manifeste ne doit contenir aucun champ password/secret"
        );

        for entry in fs::read_dir(dir.path()).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) == Some("xml") {
                let content = fs::read_to_string(&path).unwrap();
                assert!(!content.to_lowercase().contains("password"));
            }
        }
    }

    fn contains_key(value: &serde_json::Value, key: &str) -> bool {
        match value {
            serde_json::Value::Object(map) => {
                map.keys().any(|k| k.to_lowercase() == key)
                    || map.values().any(|v| contains_key(v, key))
            }
            serde_json::Value::Array(arr) => arr.iter().any(|v| contains_key(v, key)),
            _ => false,
        }
    }

    #[test]
    fn include_exclude_filters() {
        let scheduler = sample_scheduler();
        let dir = tempdir().unwrap();
        let filter = PatternFilter::new(vec!["\\Backup\\*".to_string()], vec!["*Weekly".to_string()]);
        let summary = export(&scheduler, dir.path(), true, &filter, "TESTHOST").unwrap();
        assert_eq!(summary.exported, vec!["\\Backup\\Nightly".to_string()]);
        assert!(summary.skipped_by_filter.contains(&"\\Other\\Cleanup".to_string()));
    }

    #[test]
    fn pattern_filter_glob() {
        assert!(PatternFilter::matches_one("\\Backup\\*", "\\Backup\\Nightly"));
        assert!(PatternFilter::matches_one("*Weekly", "\\Backup\\Weekly"));
        assert!(!PatternFilter::matches_one("*Weekly", "\\Backup\\Nightly"));
        assert!(PatternFilter::matches_one("\\Backup\\Nightly", "\\Backup\\Nightly"));
    }

    #[test]
    fn is_microsoft_task_detects_system_folder() {
        assert!(is_microsoft_task("\\Microsoft\\Windows\\Defrag\\ScheduledDefrag"));
        assert!(is_microsoft_task("\\microsoft\\Windows\\Update")); // insensible a la casse
        assert!(!is_microsoft_task("\\MicrosoftEdge\\Update"));
        assert!(!is_microsoft_task("\\MSIAfterburner"));
        assert!(!is_microsoft_task("\\"));
    }

    #[test]
    fn hide_microsoft_filter_excludes_system_tasks() {
        let filter = PatternFilter::hide_microsoft();
        assert!(!filter.allows("\\Microsoft\\Windows\\Defrag\\ScheduledDefrag"));
        assert!(!filter.allows("\\Microsoft\\Office\\Feature Updates"));
        assert!(filter.allows("\\MSIAfterburner"));
        assert!(filter.allows("\\Backup\\Nightly"));
    }

    #[test]
    fn hide_microsoft_filter_composes_with_include() {
        let filter = PatternFilter::new(vec!["\\Backup\\*".to_string()], vec!["\\Microsoft\\*".to_string()]);
        assert!(filter.allows("\\Backup\\Nightly"));
        assert!(!filter.allows("\\Microsoft\\Windows\\Backup"));
        assert!(!filter.allows("\\Other\\Task"));
    }

    fn task(xml: &str) -> MockTask {
        MockTask {
            xml: xml.to_string(),
            user_id: None,
            logon_type: LogonType::None,
        }
    }

    /// Regression : deux chemins de taches distincts ne doivent jamais
    /// produire le meme nom de fichier. `\A__B\T` et `\A\B__T` donnaient
    /// tous deux `A__B__T.xml` : un seul XML etait ecrit, le manifeste en
    /// referencait deux, et `load_and_verify` rejetait **toute** l'archive
    /// (empreinte SHA-256 incorrecte pour l'une des deux taches).
    #[test]
    fn deux_chemins_distincts_ne_partagent_pas_de_fichier() {
        let scheduler = MockScheduler::new(true)
            .with_task("\\A__B\\T", task("<Task>un</Task>"))
            .with_task("\\A\\B__T", task("<Task>deux</Task>"))
            .with_task("\\A_uB\\T", task("<Task>trois</Task>"))
            .with_task("\\A_u\\B", task("<Task>quatre</Task>"))
            .with_task("\\A\\B", task("<Task>cinq</Task>"));
        let dir = tempdir().unwrap();
        let filter = PatternFilter::new(vec![], vec![]);
        let summary = export(&scheduler, dir.path(), true, &filter, "HOST").unwrap();
        assert_eq!(summary.exported.len(), 5);

        let mut names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names.iter().filter(|n| n.ends_with(".xml")).count(),
            5,
            "5 XML distincts attendus, obtenu {names:?}"
        );
        assert_eq!(names.len(), 6, "5 XML + manifeste attendus, obtenu {names:?}");

        // L'archive produite doit se recharger : chaque empreinte correspond
        // bien au XML de sa propre tache (aucun fichier partage).
        let (manifest, xmls) = crate::import::load_and_verify(dir.path())
            .expect("l'archive exportee doit rester rechargeable");
        assert_eq!(manifest.tasks.len(), 5);
        for record in &manifest.tasks {
            let xml = xmls
                .get(&record.path)
                .unwrap_or_else(|| panic!("XML absent pour {}", record.path));
            assert_eq!(
                sha256_hex(xml.as_bytes()),
                record.sha256,
                "empreinte incoherente pour {}",
                record.path
            );
        }
    }

    /// Compatibilite descendante des noms : un chemin sans `_` produit
    /// exactement le nom des archives deja exportees, donc les references
    /// documentees (`\Backup\Nightly` -> `Backup__Nightly.xml`) restent
    /// vraies et les archives existantes s'importent comme avant (leur
    /// manifeste enregistre de toute facon le nom reel).
    #[test]
    fn nom_de_fichier_inchange_pour_un_chemin_sans_souligne() {
        let scheduler = MockScheduler::new(true)
            .with_task("\\Backup\\Nightly", task("<Task>n</Task>"));
        let dir = tempdir().unwrap();
        let filter = PatternFilter::new(vec![], vec![]);
        export(&scheduler, dir.path(), true, &filter, "HOST").unwrap();
        assert!(
            dir.path().join("Backup__Nightly.xml").is_file(),
            "le nom historique doit etre conserve"
        );
    }
}
