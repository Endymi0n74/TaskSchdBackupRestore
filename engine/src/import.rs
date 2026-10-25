//! Import des taches planifiees : validation du manifeste, classification
//! (creer / mettre a jour / sauter / conflit / mot de passe requis /
//! utilisateur non mappe), puis execution (ou simulation en dry-run).
//!
//! La classification est calculee une seule fois et pilote a la fois le
//! rapport dry-run et l'ecriture reelle : les deux flux partagent donc
//! exactement la meme logique de decision (exigence "dry-run identique au
//! vrai flux").

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::answers::{is_well_known_account, AnswerFile};
use crate::error::{Result, BackupError};
use crate::hash::sha256_hex;
use crate::model::{ConflictPolicy, ExecutionReport, ImportAction, LogonType, Manifest, TaskRecord};
use crate::password::PasswordResolver;
use crate::scheduler::TaskSchedulerApi;
use crate::wizard::Interactor;

/// Une ligne de plan enrichie des informations necessaires a l'ecriture
/// (utilisateur cible resolu, mot de passe si applicable). Le mot de passe
/// n'est jamais affiche : cette structure ne derive pas `Debug`.
pub struct PlanItem {
    /// Enregistrement d'origine (chemin, hash, utilisateur/logon type declares a l'export).
    pub record: TaskRecord,
    /// Chemin effectif de la tache sur la cible : `record.path` eventuellement
    /// prefixe par le dossier cible (`--folder` / `target_folder`). C'est ce
    /// chemin qui est utilise pour l'existence, la comparaison et l'ecriture.
    pub target_path: String,
    /// XML brut a enregistrer (identique a celui exporte).
    pub xml: String,
    /// Action retenue pour cette tache.
    pub action: ImportAction,
    /// Utilisateur cible resolu (apres mapping eventuel), si applicable.
    pub target_user: Option<String>,
    password: Option<String>,
}

impl PlanItem {
    /// Acces au mot de passe resolu pour cette tache, si applicable. Ne
    /// jamais journaliser cette valeur.
    pub fn take_password(&self) -> Option<&str> {
        self.password.as_deref()
    }
}

/// Options gouvernant la construction du plan d'import.
pub struct ImportOptions<'a> {
    /// Politique par defaut en cas de conflit (tache existante et differente).
    pub conflict_policy: Option<ConflictPolicy>,
    /// Ignore (sans bloquer) les taches necessitant un mot de passe non fourni.
    pub skip_password_tasks: bool,
    /// Mapping utilisateur source -> cible, fourni via `--user-map`.
    pub user_map: HashMap<String, String>,
    /// Fichier de reponses eventuellement charge, pour un import non interactif.
    pub answers: Option<&'a AnswerFile>,
    /// Dossier cible du planificateur (ex: `\\Restore-2026-01-01`) : si
    /// renseigne, chaque tache est restauree sous ce dossier en **preservant
    /// sa structure d'origine** (`\\A\\B\\Tache` -> `\\Cible\\A\\B\\Tache`).
    /// Cela evite les collisions de noms et conserve le groupement source.
    pub target_folder: Option<String>,
}

/// Calcule le chemin effectif d'une tache sur la cible.
///
/// Sans dossier cible, le chemin est inchange. Avec un dossier cible, le
/// chemin d'origine (qui commence toujours par `\\`) est prefixe :
/// `\\A\\B\\Tache` + `\\Cible` -> `\\Cible\\A\\B\\Tache`.
///
/// La structure d'origine est preservee volontairement : deux taches de meme
/// nom dans des dossiers differents ne peuvent pas entrer en collision, et le
/// groupement source est conserve. Un dossier vide ou `None` laisse le chemin
/// inchange.
pub fn remap_path(path: &str, target_folder: Option<&str>) -> String {
    let Some(folder) = target_folder.map(str::trim).filter(|f| !f.is_empty()) else {
        return path.to_string();
    };
    let folder = folder.trim_matches('\\');
    if folder.is_empty() {
        return path.to_string();
    }
    format!("\\{folder}{path}")
}

/// XML d'une tache dans l'archive : chemin relu, et **chemin canonique** qui
/// sert de cle au controle de doublon.
struct ResolvedXml {
    path: PathBuf,
    canonical: PathBuf,
}

/// Chemin du XML declare par le manifeste, valide avant lecture, avec son
/// chemin canonique.
///
/// Le manifeste est une entree **non fiable** (une archive peut venir d'une
/// autre machine ou d'une source inconnue) : `xml_file` doit rester **dans**
/// le dossier d'archive. Un chemin absolu (`C:\Windows\win.ini`), une racine
/// (`\evil.xml`) ou une remontee (`..\..\secret.xml`) sortiraient du dossier,
/// contourneraient la protection zip-slip de l'extraction et feraient lire un
/// fichier arbitraire de la machine. Les sous-dossiers internes
/// (`sous/A.xml`), eux, restent acceptes.
///
/// Le chemin canonique (`fs::canonicalize`) est celui que le systeme renvoie
/// pour le fichier : il resout ce que Win32 reinterprete (espace ou point
/// final, flux par defaut `::$DATA`, casse, separateurs `/` et `\`, liens).
/// C'est une identite de **chemin**, pas d'objet : deux liens physiques vers
/// le meme contenu resteraient distincts. Elle exige que le fichier existe —
/// ce que `validate` demandait deja (un XML absent etait une erreur d'entree/
/// sortie, meme message et meme chemin) ; son contenu n'est pas lu ici.
fn resolve_xml_file(dir: &Path, record: &TaskRecord) -> Result<ResolvedXml> {
    let rel = Path::new(&record.xml_file);
    // Tout composant doit etre un nom simple **pris au pied de la lettre** :
    // ni `:` (flux NTFS), ni espace ou point final (que Win32 supprime), ni
    // caractere interdit. Un export ne produit jamais ces formes — les noms
    // de taches Windows excluent ces caracteres et le nom de fichier se
    // termine toujours par `.xml` — donc refuser ne rejette rien de legitime.
    let mut hors_archive = rel.as_os_str().is_empty();
    let mut nom_invalide = false;
    for composant in rel.components() {
        match composant {
            Component::Normal(nom) => {
                let nom = nom.to_string_lossy();
                nom_invalide |= nom.ends_with(' ')
                    || nom.ends_with('.')
                    || nom.chars().any(|c| {
                        c.is_control() || matches!(c, ':' | '<' | '>' | '"' | '|' | '?' | '*')
                    });
            }
            _ => hors_archive = true,
        }
    }
    if hors_archive {
        return Err(BackupError::InvalidManifest(format!(
            "chemin de XML non contenu dans l'archive pour la tache '{}' : '{}'",
            record.path, record.xml_file
        )));
    }
    if nom_invalide {
        return Err(BackupError::InvalidManifest(format!(
            "nom de fichier XML invalide pour la tache '{}' : '{}'",
            record.path, record.xml_file
        )));
    }
    let path = dir.join(rel);
    let canonical = fs::canonicalize(&path).map_err(|e| BackupError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    Ok(ResolvedXml { path, canonical })
}

/// Erreur XML rattachee a une tache (chemin de tache et raison seulement).
fn invalid_xml(task: &str, reason: impl Into<String>) -> BackupError {
    BackupError::InvalidXml {
        task: task.to_string(),
        reason: reason.into(),
    }
}

/// Verifie qu'un XML de tache est un **document complet**, au sens de
/// `cli.rs` et d'AGENTS §8 : exactement **un** element racine, et hors de
/// cette racine uniquement des espaces, des commentaires ou des instructions
/// de traitement, sans aucun contenu apres elle.
///
/// Lire un seul evenement ne suffisait pas (un XML tronque passait) ; compter
/// seulement les balises ne suffisait pas non plus : `<?xml version="1.0"?>`
/// seul, `<Task/>junk` et `<Task/><Task/>` restaient declares valides par
/// `validate` (code 0) et n'echouaient qu'au `RegisterTask`, sous forme de
/// HRESULT generique. Le XML n'est jamais reecrit ici (exigence d'export
/// brut) : le document est seulement parcouru.
fn check_well_formed(xml: &str, task: &str) -> Result<()> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut depth: i32 = 0;
    let mut roots = 0usize;

    loop {
        let event = reader
            .read_event()
            .map_err(|e| invalid_xml(task, format!("XML mal forme : {e}")))?;

        match &event {
            quick_xml::events::Event::Eof => break,

            // Une racine est un element ouvert ou auto-ferme a la profondeur 0.
            quick_xml::events::Event::Start(_) => {
                if depth == 0 {
                    roots += 1;
                }
                depth += 1;
            }
            quick_xml::events::Event::Empty(_) => {
                if depth == 0 {
                    roots += 1;
                }
            }

            quick_xml::events::Event::End(_) => depth -= 1,

            // Hors racine, seuls des espaces sont admis.
            quick_xml::events::Event::Text(t) => {
                if depth == 0 {
                    let texte = t
                        .unescape()
                        .map_err(|e| invalid_xml(task, format!("texte illisible : {e}")))?;
                    if !texte.chars().all(char::is_whitespace) {
                        return Err(invalid_xml(task, "contenu texte hors de l'element racine"));
                    }
                }
            }
            quick_xml::events::Event::CData(_) if depth == 0 => {
                return Err(invalid_xml(task, "contenu CDATA hors de l'element racine"));
            }
            // Aucun export du planificateur n'utilise de declaration de type de
            // document : refusee ou qu'elle soit placee.
            quick_xml::events::Event::DocType(_) => {
                return Err(invalid_xml(task, "declaration de type de document refusee"));
            }
            // La declaration XML ne peut preceder que la racine.
            quick_xml::events::Event::Decl(_) if roots > 0 => {
                return Err(invalid_xml(task, "declaration XML apres l'element racine"));
            }

            // Commentaires et instructions de traitement : admis hors racine.
            _ => {}
        }
    }

    if depth != 0 {
        return Err(invalid_xml(task, "XML incomplet (balise non fermee)"));
    }
    match roots {
        0 => Err(invalid_xml(task, "aucun element racine")),
        1 => Ok(()),
        _ => Err(invalid_xml(task, "plusieurs elements racine")),
    }
}

/// Charge le manifeste d'un dossier d'export et verifie que chaque XML
/// correspond bien a l'empreinte enregistree lors de l'export.
///
/// Verifie aussi que chaque `xml_file` reste dans le dossier d'archive, qu'il
/// n'est utilise que par une tache (identite du systeme de fichiers), que
/// chaque chemin de tache est unique et que le XML est un document complet
/// (une seule racine, rien de non blanc autour), sans jamais le reecrire.
pub fn load_and_verify(dir: &Path) -> Result<(Manifest, HashMap<String, String>)> {
    let manifest_path = dir.join("manifest.json");
    let raw = fs::read_to_string(&manifest_path).map_err(|e| BackupError::Io {
        path: manifest_path.display().to_string(),
        source: e,
    })?;
    let manifest: Manifest = serde_json::from_str(&raw)
        .map_err(|e| BackupError::InvalidManifest(format!("{e}")))?;
    if manifest.version != Manifest::CURRENT_VERSION {
        return Err(BackupError::InvalidManifest(format!(
            "version de manifeste non supportee: {}",
            manifest.version
        )));
    }

    let mut xmls = HashMap::new();
    let mut chemins: HashSet<String> = HashSet::new();
    // Chemin canonique -> tache qui le revendique la premiere.
    let mut fichiers: HashMap<PathBuf, String> = HashMap::new();
    for record in &manifest.tasks {
        // Un chemin de tache est unique dans le planificateur (Windows ne
        // distingue pas la casse) : le meme chemin deux fois ferait restaurer
        // deux fois la meme tache, le dernier XML ecrasant le premier dans
        // `xmls`, et un echec sur le second enregistrement laisserait un etat
        // partiel. Un export ne peut pas produire un tel manifeste : refuser
        // ne rejette rien de legitime.
        if !chemins.insert(record.path.to_lowercase()) {
            return Err(BackupError::InvalidManifest(format!(
                "chemin de tache present plusieurs fois dans le manifeste : '{}'",
                record.path
            )));
        }
        let fichier = resolve_xml_file(dir, record)?;
        // Un XML porte la definition d'**une** tache : deux enregistrements
        // qui pointent vers le meme fichier restaureraient cette definition
        // sous deux noms. Un export ecrit un XML par tache, donc refuser ne
        // rejette rien de legitime.
        if let Some(premiere) = fichiers.insert(fichier.canonical, record.path.clone()) {
            return Err(BackupError::InvalidManifest(format!(
                "fichier XML utilise par plusieurs taches dans le manifeste : '{}' (taches '{}' et '{}')",
                record.xml_file, premiere, record.path
            )));
        }
        let xml = fs::read_to_string(&fichier.path).map_err(|e| BackupError::Io {
            path: fichier.path.display().to_string(),
            source: e,
        })?;
        let actual_hash = sha256_hex(xml.as_bytes());
        if actual_hash != record.sha256 {
            return Err(BackupError::HashMismatch {
                task: record.path.clone(),
            });
        }
        // Le XML doit rester analysable (bien forme) : on verifie ici sans
        // jamais le reecrire, conformement a l'exigence d'export brut.
        check_well_formed(&xml, &record.path)?;
        xmls.insert(record.path.clone(), xml);
    }
    Ok((manifest, xmls))
}

/// Determine si une resolution explicite de l'utilisateur cible est
/// necessaire avant ecriture. Les taches TASK_LOGON_PASSWORD n'ont pas
/// besoin de cette verification prealable : leur compte est valide (ou pas)
/// au moment de RegisterTask, et l'echec HRESULT correspondant est deja
/// traduit en message clair (voir error::translate_hresult). Seuls les
/// logon types sans secret stocke (S4U, jeton interactif) n'ont aucune
/// autre occasion de detecter un compte absent : on le verifie donc ici.
fn needs_mapping_check(logon_type: LogonType) -> bool {
    matches!(
        logon_type,
        LogonType::InteractiveToken | LogonType::InteractiveTokenOrPassword | LogonType::S4U
    )
}

/// Construit le plan d'import complet. C'est la meme fonction qui est
/// utilisee en dry-run et en execution reelle : seule l'etape d'ecriture
/// (`execute_plan`) differe.
#[allow(clippy::too_many_arguments)]
pub fn build_plan(
    scheduler: &dyn TaskSchedulerApi,
    manifest: &Manifest,
    xmls: &HashMap<String, String>,
    options: &ImportOptions,
    resolver: &PasswordResolver,
    interactor: &dyn Interactor,
) -> Result<Vec<PlanItem>> {
    let empty_conflicts = HashMap::new();
    let empty_skips: Vec<String> = Vec::new();
    let empty_overrides: HashMap<String, crate::action::ActionOverride> = HashMap::new();
    let (
        answer_user_map,
        answer_passwords_users,
        answer_conflicts,
        answer_skips,
        answer_overrides,
    ): (
        HashMap<String, String>,
        Vec<String>,
        &HashMap<String, crate::answers::ConflictDecision>,
        &Vec<String>,
        &HashMap<String, crate::action::ActionOverride>,
    ) = match options.answers {
        Some(a) => (
            a.user_map.clone(),
            a.passwords.keys().cloned().collect(),
            &a.conflict_decisions,
            &a.skip_tasks,
            &a.action_overrides,
        ),
        None => (
            HashMap::new(),
            Vec::new(),
            &empty_conflicts,
            &empty_skips,
            &empty_overrides,
        ),
    };
    let _ = answer_passwords_users; // le contenu des mots de passe est deja fusionne dans `resolver`

    let mut plan = Vec::new();

    for record in &manifest.tasks {
        let mut xml = xmls
            .get(&record.path)
            .ok_or_else(|| BackupError::Other(format!("XML manquant pour '{}'", record.path)))?
            .clone();
        let target_path = remap_path(&record.path, options.target_folder.as_deref());

        macro_rules! push_and_continue {
            ($action:expr) => {{
                plan.push(PlanItem {
                    record: record.clone(),
                    target_path: target_path.clone(),
                    xml,
                    action: $action,
                    target_user: None,
                    password: None,
                });
                continue;
            }};
        }

        // 1. Choix explicite de saut (answer-file), prioritaire sur tout le
        // reste — y compris sur les surcharges d'action : une tache sautee
        // n'est jamais reecrite, son XML ne doit donc pas etre touche.
        if answer_skips.iter().any(|p| p == &record.path) {
            push_and_continue!(ImportAction::SkippedByChoice);
        }

        // 2. Surcharge d'action (programme / arguments / dossier de
        // demarrage) : appliquee ici, une seule fois, avant toute
        // comparaison — le plan reflete exactement ce qui sera ecrit, en
        // simulation comme en execution reelle (interface ou helper eleve).
        if let Some(overrides) = answer_overrides.get(&record.path) {
            xml = crate::action::apply_action(&xml, &record.path, overrides)?;
        }

        // 3. Existence / conflit : determine s'il y a quoi que ce soit a
        // ecrire. Les verifications portent sur le chemin **cible** (les
        // decisions de l'utilisateur, elles, restent clees par le chemin
        // d'origine de l'archive). On ne demande mapping ou mot de passe que
        // si une ecriture est effectivement necessaire (une tache identique ou
        // explicitement sautee ne doit jamais declencher de question a
        // l'utilisateur).
        let intended_write = if !scheduler.task_exists(&target_path)? {
            Some(ImportAction::Create)
        } else {
            let existing_xml = scheduler.get_task_xml(&target_path)?;
            if existing_xml == xml {
                push_and_continue!(ImportAction::SkipIdentical);
            }
            let resolved = if let Some(decision) = answer_conflicts.get(&record.path) {
                Some(ConflictPolicy::from(*decision))
            } else if let Some(policy) = options.conflict_policy {
                Some(policy)
            } else {
                interactor.ask_conflict(&target_path)
            };
            match resolved {
                Some(ConflictPolicy::Overwrite) => Some(ImportAction::Update),
                Some(ConflictPolicy::Skip) => push_and_continue!(ImportAction::SkippedByChoice),
                None => push_and_continue!(ImportAction::Conflict),
            }
        };

        // 4. Resolution de l'utilisateur cible, uniquement pour les logon
        // types qui n'ont aucune autre occasion de detecter un compte absent.
        let mut target_user = record.user_id.clone();
        if let Some(source_user) = record.user_id.clone() {
            if needs_mapping_check(record.logon_type) && !is_well_known_account(&source_user) {
                if let Some(mapped) = options
                    .user_map
                    .get(&source_user)
                    .or_else(|| answer_user_map.get(&source_user))
                {
                    target_user = Some(mapped.clone());
                } else if let Some(mapped) = interactor.ask_user_mapping(&source_user, &target_path) {
                    target_user = Some(mapped);
                } else {
                    push_and_continue!(ImportAction::UserUnmapped { source_user });
                }
            }
        }

        // 5. Resolution du mot de passe si le logon type en necessite un.
        let mut password = None;
        if record.requires_password() {
            let user_for_pw = target_user.clone().unwrap_or_default();
            password = resolver
                .lookup_static(&user_for_pw)
                .or(resolver.prompt_interactive(&user_for_pw, &record.path)?);
            if password.is_none() {
                if options.skip_password_tasks {
                    push_and_continue!(ImportAction::SkippedByChoice);
                }
                push_and_continue!(ImportAction::PasswordRequired { user: user_for_pw });
            }
        }

        plan.push(PlanItem {
            record: record.clone(),
            target_path,
            xml,
            action: intended_write.expect("resolu ci-dessus"),
            target_user,
            password,
        });
    }

    Ok(plan)
}

/// Execute le plan sans sauvegarde d'annulation (compatibilite et tests).
///
/// Les appelants reels (CLI, interface, helper eleve) passent par
/// [`execute_plan_with_undo`] avec [`default_undo_dir`].
pub fn execute_plan(
    scheduler: &dyn TaskSchedulerApi,
    plan: &[PlanItem],
    dry_run: bool,
) -> Result<ExecutionReport> {
    execute_plan_with_undo(scheduler, plan, dry_run, None)
}

/// Dossier d'annulation par defaut : `%LOCALAPPDATA%\TaskSchdBackupRestore\undo\<horodatage>`
/// (repli sur le dossier temporaire). Sous UAC, l'elevation ne change pas le
/// profil de l'utilisateur : le helper eleve ecrit dans le meme dossier.
pub fn default_undo_dir() -> PathBuf {
    crate::data_dir()
        .join("undo")
        .join(chrono::Local::now().format("%Y%m%d-%H%M%S").to_string())
}

/// Execute le plan. En dry-run, aucune ecriture n'est effectuee (ni tache, ni
/// sauvegarde) : le rapport produit reste identique, seule l'ecriture reelle
/// est court-circuitee.
///
/// Rendre l'ecrasement reversible : avant de remplacer une tache existante,
/// son XML **actuel** est ecrit dans `undo_dir`, qui forme une archive
/// reimportable (`TaskSchdBackupRestore import <undo_dir>`). Si cette sauvegarde echoue, la
/// tache n'est **pas** ecrasee : on ne perd jamais une definition faute
/// d'avoir pu la copier, et l'echec est remonte dans le rapport.
pub fn execute_plan_with_undo(
    scheduler: &dyn TaskSchedulerApi,
    plan: &[PlanItem],
    dry_run: bool,
    undo_dir: Option<&Path>,
) -> Result<ExecutionReport> {
    let mut report = ExecutionReport::default();
    let mut sauvegardes: Vec<(String, String, String)> = Vec::new();

    if !dry_run && plan.iter().any(|p| p.action.is_writing()) && !scheduler.has_write_privileges()
    {
        return Err(BackupError::AccessDenied {
            operation: "import (RegisterTask)".to_string(),
        });
    }

    for item in plan {
        match &item.action {
            ImportAction::Create | ImportAction::Update => {
                if dry_run {
                    match item.action {
                        ImportAction::Create => report.created.push(item.target_path.clone()),
                        ImportAction::Update => report.updated.push(item.target_path.clone()),
                        _ => unreachable!(),
                    }
                    continue;
                }
                // Ecrasement : copier d'abord la definition actuelle.
                if item.action == ImportAction::Update {
                    if let Some(dir) = undo_dir {
                        match sauvegarder_tache(scheduler, dir, &item.target_path) {
                            Ok((fichier, xml)) => {
                                sauvegardes.push((item.target_path.clone(), fichier, xml));
                            }
                            Err(e) => {
                                report.failed.push((
                                    item.target_path.clone(),
                                    format!("sauvegarde impossible, tache laissee intacte : {e}"),
                                ));
                                continue;
                            }
                        }
                    }
                }
                let folder = crate::scheduler::parent_folder(&item.target_path);
                if let Err(e) = scheduler.ensure_folder(&folder) {
                    report.failed.push((item.target_path.clone(), e.to_string()));
                    continue;
                }
                let result = scheduler.register_task(
                    &item.target_path,
                    &item.xml,
                    item.target_user.as_deref(),
                    item.take_password(),
                    item.record.logon_type,
                );
                match result {
                    Ok(()) => match item.action {
                        ImportAction::Create => report.created.push(item.target_path.clone()),
                        ImportAction::Update => report.updated.push(item.target_path.clone()),
                        _ => unreachable!(),
                    },
                    Err(e) => report.failed.push((item.target_path.clone(), e.to_string())),
                }
            }
            ImportAction::SkipIdentical | ImportAction::SkippedByChoice => {
                report.skipped.push(item.target_path.clone());
            }
            ImportAction::Conflict => {
                report
                    .blocked
                    .push((item.target_path.clone(), "conflit non resolu".to_string()));
            }
            ImportAction::PasswordRequired { user } => {
                report.blocked.push((
                    item.target_path.clone(),
                    format!("mot de passe requis pour '{user}'"),
                ));
            }
            ImportAction::UserUnmapped { source_user } => {
                report.blocked.push((
                    item.target_path.clone(),
                    format!("utilisateur '{source_user}' non mappe"),
                ));
            }
        }
    }

    // Manifeste du dossier d'annulation : les XML sauvegardes forment une
    // archive valide, donc reimportables telles quelles (`TaskSchdBackupRestore validate`).
    if let (Some(dir), false) = (undo_dir, sauvegardes.is_empty()) {
        match ecrire_manifeste_undo(dir, &sauvegardes) {
            Ok(()) => report.backed_up = sauvegardes.iter().map(|(_, f, _)| f.clone()).collect(),
            Err(e) => report.failed.push((
                dir.display().to_string(),
                format!("manifeste du dossier d'annulation non ecrit : {e}"),
            )),
        }
    }

    Ok(report)
}

/// Copie le XML actuel d'une tache dans `dir` et retourne (nom de fichier, XML).
///
/// Le nom suit le codage injectif de l'export (une tache = un fichier) ; en
/// cas de doublon — deux executions dans le meme dossier — un suffixe
/// numerique evite d'ecraser une sauvegarde precedente.
fn sauvegarder_tache(
    scheduler: &dyn TaskSchedulerApi,
    dir: &Path,
    task: &str,
) -> Result<(String, String)> {
    let xml = scheduler.get_task_xml(task)?;
    fs::create_dir_all(dir).map_err(|e| BackupError::Io {
        path: dir.display().to_string(),
        source: e,
    })?;
    let base = crate::export::sanitize_filename(task);
    let mut fichier = format!("{base}.xml");
    let mut n = 2;
    while dir.join(&fichier).exists() {
        fichier = format!("{base}-{n}.xml");
        n += 1;
    }
    fs::write(dir.join(&fichier), xml.as_bytes()).map_err(|e| BackupError::Io {
        path: dir.join(&fichier).display().to_string(),
        source: e,
    })?;
    Ok((fichier, xml))
}

/// Ecrit le `manifest.json` du dossier d'annulation (archive reimportable).
fn ecrire_manifeste_undo(dir: &Path, sauvegardes: &[(String, String, String)]) -> Result<()> {
    let records = sauvegardes
        .iter()
        .map(|(path, fichier, xml)| TaskRecord {
            path: path.clone(),
            xml_file: fichier.clone(),
            sha256: sha256_hex(xml.as_bytes()),
            // Type de connexion inconnu ici : le XML sauvegardé porte le
            // principal d'origine, et une tache a mot de passe redemandera le
            // secret a la reimportation comme n'importe quelle archive.
            user_id: None,
            logon_type: LogonType::None,
        })
        .collect();
    let manifest = Manifest::new(crate::local_host_name(), records);
    let path = dir.join("manifest.json");
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| BackupError::Other(format!("manifeste d'annulation : {e}")))?;
    fs::write(&path, json).map_err(|e| BackupError::Io {
        path: path.display().to_string(),
        source: e,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ActionOverride;
    use crate::model::{LogonType, TaskRecord};
    use crate::scheduler::mock::{MockScheduler, MockTask};
    use crate::wizard::NullInteractor;

    fn record(path: &str, logon: LogonType, user: Option<&str>, xml: &str) -> (TaskRecord, String) {
        (
            TaskRecord {
                path: path.to_string(),
                xml_file: format!("{}.xml", path.replace('\\', "__")),
                sha256: sha256_hex(xml.as_bytes()),
                user_id: user.map(|s| s.to_string()),
                logon_type: logon,
            },
            xml.to_string(),
        )
    }

    fn manifest_and_xmls(entries: Vec<(TaskRecord, String)>) -> (Manifest, HashMap<String, String>) {
        let mut xmls = HashMap::new();
        let mut records = Vec::new();
        for (r, xml) in entries {
            xmls.insert(r.path.clone(), xml);
            records.push(r);
        }
        (Manifest::new("TESTHOST".to_string(), records), xmls)
    }

    #[test]
    fn creates_new_task_without_secret() {
        let (r, xml) = record("\\A\\NoSecret", LogonType::None, None, "<Task>a</Task>");
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true);
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].action, ImportAction::Create);

        let report = execute_plan(&scheduler, &plan, false).unwrap();
        assert_eq!(report.created, vec!["\\A\\NoSecret".to_string()]);
        assert_eq!(report.exit_code(), 0);
        assert_eq!(scheduler.registration_count(), 1);
    }

    #[test]
    fn dry_run_never_writes() {
        let (r, xml) = record("\\A\\NoSecret", LogonType::None, None, "<Task>a</Task>");
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true);
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        let report = execute_plan(&scheduler, &plan, true).unwrap();
        assert_eq!(report.created, vec!["\\A\\NoSecret".to_string()]);
        assert_eq!(scheduler.registration_count(), 0, "dry-run ne doit rien ecrire");
    }

    #[test]
    fn identical_existing_task_is_skipped() {
        let xml = "<Task>same</Task>";
        let (r, xml_owned) = record("\\A\\Same", LogonType::None, None, xml);
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml_owned)]);
        let scheduler = MockScheduler::new(true).with_task(
            "\\A\\Same",
            MockTask {
                xml: xml.to_string(),
                user_id: None,
                logon_type: LogonType::None,
            },
        );
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].action, ImportAction::SkipIdentical);
        let report = execute_plan(&scheduler, &plan, false).unwrap();
        assert_eq!(report.skipped, vec!["\\A\\Same".to_string()]);
        assert_eq!(scheduler.registration_count(), 0);
    }

    #[test]
    fn differing_existing_task_without_policy_blocks() {
        let (r, xml) = record("\\A\\Diff", LogonType::None, None, "<Task>new</Task>");
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true).with_task(
            "\\A\\Diff",
            MockTask {
                xml: "<Task>old</Task>".to_string(),
                user_id: None,
                logon_type: LogonType::None,
            },
        );
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].action, ImportAction::Conflict);
        let report = execute_plan(&scheduler, &plan, false).unwrap();
        assert_eq!(report.blocked.len(), 1);
        assert_eq!(report.exit_code(), 1);
    }

    #[test]
    fn conflict_policy_overwrite_updates() {
        let (r, xml) = record("\\A\\Diff", LogonType::None, None, "<Task>new</Task>");
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true).with_task(
            "\\A\\Diff",
            MockTask {
                xml: "<Task>old</Task>".to_string(),
                user_id: None,
                logon_type: LogonType::None,
            },
        );
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: Some(ConflictPolicy::Overwrite),
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].action, ImportAction::Update);
        let report = execute_plan(&scheduler, &plan, false).unwrap();
        assert_eq!(report.updated, vec!["\\A\\Diff".to_string()]);
        assert_eq!(report.exit_code(), 0);
    }

    #[test]
    fn password_required_blocks_without_source() {
        let (r, xml) = record(
            "\\A\\WithPw",
            LogonType::Password,
            Some("DOMAIN\\alice"),
            "<Task>pw</Task>",
        );
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true);
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        match &plan[0].action {
            ImportAction::PasswordRequired { user } => assert_eq!(user, "DOMAIN\\alice"),
            other => panic!("expected PasswordRequired, got {other:?}"),
        }
        let report = execute_plan(&scheduler, &plan, false).unwrap();
        assert_eq!(report.exit_code(), 1);
    }

    #[test]
    fn skip_password_tasks_flag_skips_instead_of_blocking() {
        let (r, xml) = record(
            "\\A\\WithPw",
            LogonType::Password,
            Some("DOMAIN\\alice"),
            "<Task>pw</Task>",
        );
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true);
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: true,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].action, ImportAction::SkippedByChoice);
        let report = execute_plan(&scheduler, &plan, false).unwrap();
        assert_eq!(report.skipped, vec!["\\A\\WithPw".to_string()]);
        assert_eq!(report.exit_code(), 0);
    }

    #[test]
    fn password_file_supplies_secret_and_task_is_created() {
        let (r, xml) = record(
            "\\A\\WithPw",
            LogonType::Password,
            Some("DOMAIN\\alice"),
            "<Task>pw</Task>",
        );
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true);
        let mut pf = HashMap::new();
        pf.insert("domain\\alice".to_string(), "s3cret".to_string());
        let resolver = PasswordResolver::new(Some(pf), None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].action, ImportAction::Create);
        let report = execute_plan(&scheduler, &plan, false).unwrap();
        assert_eq!(report.created, vec!["\\A\\WithPw".to_string()]);
    }

    #[test]
    fn unmapped_user_blocks_without_user_map() {
        let (r, xml) = record(
            "\\A\\S4u",
            LogonType::S4U,
            Some("OLDPC\\bob"),
            "<Task>s4u</Task>",
        );
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true);
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        match &plan[0].action {
            ImportAction::UserUnmapped { source_user } => assert_eq!(source_user, "OLDPC\\bob"),
            other => panic!("expected UserUnmapped, got {other:?}"),
        }
    }

    #[test]
    fn user_map_resolves_and_creates() {
        let (r, xml) = record(
            "\\A\\S4u",
            LogonType::S4U,
            Some("OLDPC\\bob"),
            "<Task>s4u</Task>",
        );
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true);
        let resolver = PasswordResolver::new(None, None, false);
        let mut user_map = HashMap::new();
        user_map.insert("OLDPC\\bob".to_string(), "NEWPC\\bob".to_string());
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map,
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].action, ImportAction::Create);
        assert_eq!(plan[0].target_user, Some("NEWPC\\bob".to_string()));
    }

    #[test]
    fn well_known_account_needs_no_mapping() {
        let (r, xml) = record("\\A\\Sys", LogonType::S4U, Some("SYSTEM"), "<Task>sys</Task>");
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true);
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].action, ImportAction::Create);
    }

    #[test]
    fn access_denied_when_no_write_privileges() {
        let (r, xml) = record("\\A\\NoSecret", LogonType::None, None, "<Task>a</Task>");
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(false);
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        let result = execute_plan(&scheduler, &plan, false);
        assert!(matches!(result, Err(BackupError::AccessDenied { .. })));
    }

    #[test]
    fn remap_path_without_folder_is_identity() {
        assert_eq!(remap_path("\\A\\B\\Task", None), "\\A\\B\\Task");
        assert_eq!(remap_path("\\A\\B\\Task", Some("")), "\\A\\B\\Task");
        assert_eq!(remap_path("\\A\\B\\Task", Some("  ")), "\\A\\B\\Task");
        assert_eq!(remap_path("\\A\\B\\Task", Some("\\")), "\\A\\B\\Task");
    }

    #[test]
    fn remap_path_prefixes_while_preserving_structure() {
        assert_eq!(remap_path("\\Task", Some("\\Restore")), "\\Restore\\Task");
        assert_eq!(
            remap_path("\\A\\B\\Task", Some("\\Restore-2026-01-01")),
            "\\Restore-2026-01-01\\A\\B\\Task"
        );
        // Les slashes superflus du dossier sont nettoyes, la structure source conservee.
        assert_eq!(remap_path("\\A\\Task", Some("Restore\\")), "\\Restore\\A\\Task");
    }

    #[test]
    fn target_folder_prefixes_creation_and_report() {
        let (r, xml) = record("\\A\\NoSecret", LogonType::None, None, "<Task>a</Task>");
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true);
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: Some("\\Restore".to_string()),
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].target_path, "\\Restore\\A\\NoSecret");
        assert_eq!(plan[0].action, ImportAction::Create);

        let report = execute_plan(&scheduler, &plan, false).unwrap();
        assert_eq!(report.created, vec!["\\Restore\\A\\NoSecret".to_string()]);
        assert_eq!(report.exit_code(), 0);
        // La tache est bien enregistree sous le chemin cible, pas le chemin source.
        assert!(scheduler.task_exists("\\Restore\\A\\NoSecret").unwrap());
        assert!(!scheduler.task_exists("\\A\\NoSecret").unwrap());
    }

    #[test]
    fn target_folder_skips_identical_under_remapped_path() {
        // Une tache deja presente **sous le chemin cible** avec le meme XML
        // doit etre classee identique (et non recreee sous un chemin different).
        let xml = "<Task>same</Task>";
        let (r, xml_owned) = record("\\A\\Same", LogonType::None, None, xml);
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml_owned)]);
        let scheduler = MockScheduler::new(true).with_task(
            "\\Restore\\A\\Same",
            MockTask {
                xml: xml.to_string(),
                user_id: None,
                logon_type: LogonType::None,
            },
        );
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: Some("\\Restore".to_string()),
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].action, ImportAction::SkipIdentical);
        assert_eq!(plan[0].target_path, "\\Restore\\A\\Same");
        let report = execute_plan(&scheduler, &plan, false).unwrap();
        assert_eq!(report.skipped, vec!["\\Restore\\A\\Same".to_string()]);
        assert_eq!(scheduler.registration_count(), 0);
    }

    #[test]
    fn action_override_reecrit_le_xml_du_plan_et_de_l_ecriture() {
        let xml = r#"<Task><Actions><Exec><Command>a.exe</Command></Exec></Actions></Task>"#;
        let (r, xml_owned) = record("\\A\\Job", LogonType::None, None, xml);
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml_owned)]);
        let scheduler = MockScheduler::new(true);
        let resolver = PasswordResolver::new(None, None, false);

        let mut answers = AnswerFile::default();
        answers.action_overrides.insert(
            "\\A\\Job".to_string(),
            ActionOverride {
                command: Some(r"D:\Scripts\a.exe".to_string()),
                ..Default::default()
            },
        );
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: Some(&answers),
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].action, ImportAction::Create);
        assert!(
            plan[0].xml.contains(r"<Command>D:\Scripts\a.exe</Command>"),
            "le plan porte le XML modifie: {}",
            plan[0].xml
        );

        let report = execute_plan(&scheduler, &plan, false).unwrap();
        assert_eq!(report.exit_code(), 0);
        let written = scheduler.get_task_xml("\\A\\Job").unwrap();
        assert!(written.contains(r"D:\Scripts\a.exe"), "ecriture avec la surcharge");
        assert!(!written.contains("<Command>a.exe"), "l'ancien programme ne subsiste pas");
    }

    #[test]
    fn action_override_rend_la_tache_existante_differente() {
        // La tache existante est identique a l'archive : sans surcharge,
        // rien a faire ; avec surcharge, le XML effectif differe desormais.
        let xml = r#"<Task><Actions><Exec><Command>a.exe</Command></Exec></Actions></Task>"#;
        let (r, xml_owned) = record("\\A\\Job", LogonType::None, None, xml);
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml_owned)]);
        let scheduler = MockScheduler::new(true).with_task(
            "\\A\\Job",
            MockTask {
                xml: xml.to_string(),
                user_id: None,
                logon_type: LogonType::None,
            },
        );
        let resolver = PasswordResolver::new(None, None, false);

        let sans = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &sans, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].action, ImportAction::SkipIdentical);

        let mut answers = AnswerFile::default();
        answers.action_overrides.insert(
            "\\A\\Job".to_string(),
            ActionOverride {
                command: Some("new.exe".to_string()),
                ..Default::default()
            },
        );
        let avec = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: Some(&answers),
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &avec, &resolver, &NullInteractor).unwrap();
        assert_eq!(
            plan[0].action,
            ImportAction::Conflict,
            "le XML modifie n'est plus identique a l'existant"
        );

        let ecrase = ImportOptions {
            conflict_policy: Some(ConflictPolicy::Overwrite),
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: Some(&answers),
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &ecrase, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].action, ImportAction::Update);
        let report = execute_plan(&scheduler, &plan, false).unwrap();
        assert_eq!(report.updated, vec!["\\A\\Job".to_string()]);
    }

    #[test]
    fn action_override_identique_a_l_existante_est_classee_identique() {
        // Cas inverse : l'existant correspond deja au XML modifie (tache
        // configuree sur la cible avec les nouveaux chemins).
        let original = r#"<Task><Actions><Exec><Command>old</Command></Exec></Actions></Task>"#;
        let (r, xml_owned) = record("\\A\\Job", LogonType::None, None, original);
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml_owned)]);
        let scheduler = MockScheduler::new(true).with_task(
            "\\A\\Job",
            MockTask {
                xml: r#"<Task><Actions><Exec><Command>new</Command></Exec></Actions></Task>"#
                    .to_string(),
                user_id: None,
                logon_type: LogonType::None,
            },
        );
        let resolver = PasswordResolver::new(None, None, false);

        let mut answers = AnswerFile::default();
        answers.action_overrides.insert(
            "\\A\\Job".to_string(),
            ActionOverride {
                command: Some("new".to_string()),
                ..Default::default()
            },
        );
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: Some(&answers),
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].action, ImportAction::SkipIdentical);
    }

    #[test]
    fn action_override_sans_action_exec_fait_echouer_le_plan() {
        let xml = r#"<Task><Actions><ComHandler><Command>{c}</Command></ComHandler></Actions></Task>"#;
        let (r, xml_owned) = record("\\A\\Job", LogonType::None, None, xml);
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml_owned)]);
        let scheduler = MockScheduler::new(true);
        let resolver = PasswordResolver::new(None, None, false);

        let mut answers = AnswerFile::default();
        answers.action_overrides.insert(
            "\\A\\Job".to_string(),
            ActionOverride {
                command: Some("job.exe".to_string()),
                ..Default::default()
            },
        );
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: Some(&answers),
            target_folder: None,
        };
        // Pas de `unwrap_err()` ici : `PlanItem` ne derive volontairement
        // pas `Debug` (il peut porter un mot de passe).
        let err = match build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor) {
            Err(e) => e,
            Ok(_) => panic!("le plan doit echouer sur une surcharge sans action Exec"),
        };
        assert!(
            err.to_string().contains("aucune action Exec"),
            "message clair: {err}"
        );
    }

    #[test]
    fn hash_mismatch_is_detected_on_load() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = Manifest::new(
            "H".to_string(),
            vec![TaskRecord {
                path: "\\A\\B".to_string(),
                xml_file: "A__B.xml".to_string(),
                sha256: "0000000000000000000000000000000000000000000000000000000000000".to_string(),
                user_id: None,
                logon_type: LogonType::None,
            }],
        );
        std::fs::write(
            dir.path().join("manifest.json"),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();
        std::fs::write(dir.path().join("A__B.xml"), "<Task/>").unwrap();
        let result = load_and_verify(dir.path());
        assert!(matches!(result, Err(BackupError::HashMismatch { .. })));
    }

    /// Ecrit un dossier d'export minimal : un `manifest.json` decrivant une
    /// seule tache `\A` (XML fourni) pointant vers `xml_file`, l'empreinte
    /// etant calculee sur ce XML.
    fn write_archive(dir: &Path, xml_file: &str, xml: &str) {
        let manifest = Manifest::new(
            "H".to_string(),
            vec![TaskRecord {
                path: "\\A".to_string(),
                xml_file: xml_file.to_string(),
                sha256: sha256_hex(xml.as_bytes()),
                user_id: None,
                logon_type: LogonType::None,
            }],
        );
        std::fs::write(
            dir.join("manifest.json"),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();
    }

    /// Regression : le `xml_file` du manifeste est une entree non fiable. Un
    /// chemin absolu, une racine ou une remontee `..` sortent du dossier
    /// d'archive et doivent etre refuses : sinon `validate` et `import` lisent
    /// un fichier arbitraire de la machine (ex. `C:\Windows\win.ini`) et la
    /// protection zip-slip de l'extraction est contournee par le manifeste.
    #[test]
    fn xml_file_sortant_de_l_archive_est_refuse() {
        for bad in [r"..\..\secret.xml", r"C:\Windows\win.ini", r"\evil.xml", ""] {
            let dir = tempfile::tempdir().unwrap();
            write_archive(dir.path(), bad, "<Task/>");
            let err = load_and_verify(dir.path())
                .err()
                .unwrap_or_else(|| panic!("chemin '{bad}' accepte a tort"));
            assert!(
                matches!(err, BackupError::InvalidManifest(_)),
                "erreur inattendue pour '{bad}': {err}"
            );
        }
    }

    /// Regression : deux enregistrements pour le **meme** chemin de tache
    /// etaient acceptes (« Archive valide : 2 tache(s) », code 0) et le plan
    /// produisait deux `CREATE` sur la meme tache — travail duplique au
    /// retour, et etat partiel si le second enregistrement echouait. Un export
    /// ne peut pas produire un tel manifeste, donc rien de legitime n'est
    /// refuse. La casse est ignoree : Windows ne distingue pas `\A` de `\a`.
    #[test]
    fn chemin_de_tache_duplique_dans_le_manifeste_est_refuse() {
        for (premier, second) in [("\\A", "\\A"), ("\\A", "\\a")] {
            let dir = tempfile::tempdir().unwrap();
            let tasks: Vec<TaskRecord> = [("A.xml", "<Task>un</Task>"), ("B.xml", "<Task>deux</Task>")]
                .into_iter()
                .zip([premier, second])
                .map(|((fichier, xml), chemin)| {
                    std::fs::write(dir.path().join(fichier), xml).unwrap();
                    TaskRecord {
                        path: chemin.to_string(),
                        xml_file: fichier.to_string(),
                        sha256: sha256_hex(xml.as_bytes()),
                        user_id: None,
                        logon_type: LogonType::None,
                    }
                })
                .collect();
            std::fs::write(
                dir.path().join("manifest.json"),
                serde_json::to_string(&Manifest::new("H".to_string(), tasks)).unwrap(),
            )
            .unwrap();

            let err = load_and_verify(dir.path())
                .err()
                .unwrap_or_else(|| panic!("'{premier}' + '{second}' accepte a tort"));
            match err {
                BackupError::InvalidManifest(m) => assert!(
                    m.contains(second),
                    "le message doit nommer le chemin duplique, obtenu : {m}"
                ),
                autre => panic!("erreur inattendue pour '{second}': {autre}"),
            }
        }
    }

    /// Regression : deux chemins de tache **differents** pointant vers le meme
    /// `xml_file` etaient acceptes (code 0) : la definition d'une tache etait
    /// alors restauree sous deux noms. Un export ecrit un XML par tache, donc
    /// refuser ne rejette rien de legitime.
    ///
    /// L'identite est celle du **chemin canonique** : la casse et les
    /// separateurs `/` et `\` sont ramenes au meme fichier.
    #[test]
    fn xml_partage_entre_deux_taches_est_refuse() {
        // (fichier reellement ecrit, deux enregistrements (chemin, xml_file))
        for (fichier, enregistrements) in [
            ("A.xml", [("\\A", "A.xml"), ("\\B", "A.xml")]),
            ("A.xml", [("\\A", "A.xml"), ("\\B", "A.XML")]),
            ("sous/A.xml", [("\\A", "sous/A.xml"), ("\\B", "sous\\A.xml")]),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let chemin = dir.path().join(fichier);
            std::fs::create_dir_all(chemin.parent().unwrap()).unwrap();
            let xml = "<Task>une seule definition</Task>";
            std::fs::write(&chemin, xml).unwrap();

            let tasks: Vec<TaskRecord> = enregistrements
                .iter()
                .map(|(path, xml_file)| TaskRecord {
                    path: (*path).to_string(),
                    xml_file: (*xml_file).to_string(),
                    sha256: sha256_hex(xml.as_bytes()),
                    user_id: None,
                    logon_type: LogonType::None,
                })
                .collect();
            std::fs::write(
                dir.path().join("manifest.json"),
                serde_json::to_string(&Manifest::new("H".to_string(), tasks)).unwrap(),
            )
            .unwrap();

            let partage = enregistrements[1].1;
            let err = load_and_verify(dir.path())
                .err()
                .unwrap_or_else(|| panic!("'{partage}' partage accepte a tort"));
            match err {
                BackupError::InvalidManifest(m) => {
                    assert!(
                        m.contains(partage),
                        "le message doit nommer le fichier en cause, obtenu : {m}"
                    );
                    assert!(
                        m.contains("\\A") && m.contains("\\B"),
                        "le message doit nommer les deux taches, obtenu : {m}"
                    );
                }
                autre => panic!("erreur inattendue pour '{partage}': {autre}"),
            }
        }
    }

    /// Un `xml_file` que Win32 reinterprete est refuse **au nom** : espace ou
    /// point final (supprimes), `:` (flux NTFS), caracteres interdits ou de
    /// controle. Une liste de normalisations ecrite a la main laissait passer
    /// `A.xml `, `A.xml.` et `A.xml::$DATA` ; ici c'est une seule regle, et
    /// elle est verifiee avant tout acces disque.
    #[test]
    fn nom_de_fichier_xml_invalide_est_refuse() {
        for mauvais in [
            "A.xml ",        // espace final : supprime par Win32
            "A.xml.",        // point final : supprime par Win32
            "A.xml::$DATA",  // flux NTFS par defaut
            "A?.xml",
            "A*.xml",
            "A<.xml",
            "A>.xml",
            "A\".xml",
            "A|.xml",
            "A\u{1}.xml",
        ] {
            let dir = tempfile::tempdir().unwrap();
            write_archive(dir.path(), mauvais, "<Task/>");
            let err = load_and_verify(dir.path())
                .err()
                .unwrap_or_else(|| panic!("nom '{mauvais}' accepte a tort"));
            match err {
                BackupError::InvalidManifest(m) => assert!(
                    m.contains("nom de fichier XML invalide"),
                    "message inattendu pour '{mauvais}' : {m}"
                ),
                autre => panic!("erreur inattendue pour '{mauvais}': {autre}"),
            }
        }
    }

    /// Deux XML reellement distincts restent acceptes : l'identite du systeme
    /// de fichiers ne doit pas confondre deux fichiers differents, y compris
    /// dans un sous-dossier (arborescence preservee par l'extraction).
    #[test]
    fn deux_xml_distincts_restent_acceptes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("sous")).unwrap();
        let xmls = [
            ("\\A", "A.xml", "<Task>a</Task>"),
            ("\\B", "sous/B.xml", "<Task>b</Task>"),
            // Espaces, parentheses et points internes restent des noms valides.
            (
                "\\C",
                "Mes Taches__Sauvegarde (quotidienne) v2.1.xml",
                "<Task>c</Task>",
            ),
        ];
        let tasks: Vec<TaskRecord> = xmls
            .iter()
            .map(|(path, xml_file, xml)| {
                std::fs::write(dir.path().join(xml_file), xml).unwrap();
                TaskRecord {
                    path: (*path).to_string(),
                    xml_file: (*xml_file).to_string(),
                    sha256: sha256_hex(xml.as_bytes()),
                    user_id: None,
                    logon_type: LogonType::None,
                }
            })
            .collect();
        std::fs::write(
            dir.path().join("manifest.json"),
            serde_json::to_string(&Manifest::new("H".to_string(), tasks)).unwrap(),
        )
        .unwrap();

        let (manifest, xmls) = load_and_verify(dir.path()).expect("fichiers distincts acceptes");
        assert_eq!(manifest.tasks.len(), 3);
        assert_eq!(xmls.len(), 3);
    }

    /// Regression : `validate` doit honorer ce que `cli.rs` et AGENTS §8
    /// promettent — un document XML **complet**. Avant ce correctif, ces
    /// documents sortaient « Archive valide » en code 0 : declaration seule
    /// (aucune racine), contenu apres la racine, deux racines, contenu avant
    /// la racine, texte apres un commentaire, CDATA hors racine.
    #[test]
    fn document_xml_incomplet_est_refuse() {
        for (xml, motif) in [
            (r#"<?xml version="1.0" encoding="UTF-8"?>"#, "aucun element racine"),
            ("<Task/>junk", "hors de l'element racine"),
            ("<Task/><Task/>", "plusieurs elements racine"),
            ("<Task/><![CDATA[x]]>", "CDATA"),
            ("<!DOCTYPE Task><Task/>", "type de document"),
            ("<Task><!DOCTYPE x></Task>", "type de document"),
            (r#"<Task/><?xml version="1.0"?>"#, "declaration XML apres"),
        ] {
            let dir = tempfile::tempdir().unwrap();
            write_archive(dir.path(), "T.xml", xml);
            std::fs::write(dir.path().join("T.xml"), xml).unwrap();

            let err = load_and_verify(dir.path())
                .err()
                .unwrap_or_else(|| panic!("document '{xml}' accepte a tort"));
            match err {
                BackupError::InvalidXml { reason, .. } => assert!(
                    reason.contains(motif),
                    "motif '{motif}' absent de '{reason}' pour '{xml}'"
                ),
                autre => panic!("erreur inattendue pour '{xml}': {autre}"),
            }
        }
    }

    /// Un document tel que le planificateur l'exporte (declaration UTF-16,
    /// commentaire, indentation, espaces avant et apres la racine) doit
    /// rester valide : le controle ne doit pas devenir plus strict que la
    /// realite des exports (verifie sur 173 taches reelles de la machine).
    #[test]
    fn document_xml_complet_reste_accepte() {
        let xml = r#"<?xml version="1.0" encoding="UTF-16"?>
<!-- exporte par TaskSchdBackupRestore -->
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Actions><Exec><Command>C:\job.exe</Command></Exec></Actions>
</Task>
"#;
        let dir = tempfile::tempdir().unwrap();
        write_archive(dir.path(), "T.xml", xml);
        std::fs::write(dir.path().join("T.xml"), xml).unwrap();

        let (manifest, xmls) = load_and_verify(dir.path()).expect("document complet accepte");
        assert_eq!(manifest.tasks.len(), 1);
        assert_eq!(
            xmls.get("\\A").map(String::as_str),
            Some(xml),
            "le XML n'est jamais reecrit"
        );
    }

    /// Regression : la verification « XML bien forme » ne lisait qu'un seul
    /// evenement (`read_event`), donc un document tronque ou des balises
    /// croisees passaient. L'archive etait declaree valide par `validate`
    /// puis echouait seulement au `RegisterTask`, avec un HRESULT generique.
    #[test]
    fn xml_mal_forme_est_refuse_au_chargement() {
        for bad in [
            "<Task><Actions><Exec><Command>a.exe</Command>",
            "<Task></Tasks>",
        ] {
            let dir = tempfile::tempdir().unwrap();
            write_archive(dir.path(), "A__B.xml", bad);
            std::fs::write(dir.path().join("A__B.xml"), bad).unwrap();
            let err = load_and_verify(dir.path())
                .err()
                .unwrap_or_else(|| panic!("XML '{bad}' accepte a tort"));
            assert!(
                matches!(err, BackupError::InvalidXml { .. }),
                "erreur inattendue pour '{bad}': {err}"
            );
        }
    }

    /// Plan d'ecrasement : la tache `\A` existe deja avec un autre XML.
    fn plan_d_ecrasement(scheduler: &MockScheduler) -> Vec<PlanItem> {
        let (r, xml) = record("\\A", LogonType::None, None, "<Task>neuf</Task>");
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let options = ImportOptions {
            conflict_policy: Some(ConflictPolicy::Overwrite),
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let resolver = PasswordResolver::new(None, None, false);
        let plan = build_plan(scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor)
            .unwrap();
        assert_eq!(plan[0].action, ImportAction::Update);
        plan
    }

    fn scheduler_avec_tache_existante() -> MockScheduler {
        MockScheduler::new(true).with_task(
            "\\A",
            MockTask {
                xml: "<Task>vieux</Task>".to_string(),
                user_id: None,
                logon_type: LogonType::None,
            },
        )
    }

    /// Regression : ecraser une tache detruisait sa definition sans retour.
    /// Le XML **actuel** est desormais copie avant l'ecriture, dans un dossier
    /// qui forme une archive valide — donc reimportable tel quel.
    #[test]
    fn ecrasement_sauvegarde_le_xml_actuel() {
        let scheduler = scheduler_avec_tache_existante();
        let plan = plan_d_ecrasement(&scheduler);

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("undo");
        let report = execute_plan_with_undo(&scheduler, &plan, false, Some(&dir)).unwrap();

        assert_eq!(report.updated, vec!["\\A".to_string()]);
        assert_eq!(scheduler.registration_count(), 1);
        assert_eq!(report.backed_up.len(), 1, "une sauvegarde ecrite");

        // Le dossier d'annulation se recharge comme une archive, et il porte
        // l'ANCIENNE definition, pas la nouvelle.
        let (manifest, sauvegardes) =
            load_and_verify(&dir).expect("dossier d'annulation reimportable");
        assert_eq!(manifest.tasks.len(), 1);
        assert_eq!(
            sauvegardes.get("\\A").map(String::as_str),
            Some("<Task>vieux</Task>")
        );
    }

    /// Bout en bout : une restauration qui ecrase une tache peut etre annulee
    /// en reimportant le dossier de sauvegarde, comme le ferait
    /// `TaskSchdBackupRestore import <dossier>`. La tache retrouve **exactement** son XML
    /// d'origine.
    #[test]
    fn annulation_reinstalle_le_xml_ecrase() {
        let scheduler = scheduler_avec_tache_existante();
        let plan = plan_d_ecrasement(&scheduler);

        let tmp = tempfile::tempdir().unwrap();
        let undo = tmp.path().join("undo");
        let report = execute_plan_with_undo(&scheduler, &plan, false, Some(&undo)).unwrap();
        assert_eq!(report.updated, vec!["\\A".to_string()]);
        assert_eq!(
            scheduler.get_task_xml("\\A").unwrap(),
            "<Task>neuf</Task>",
            "la restauration a bien ecrase la tache"
        );

        // Annulation : le dossier de sauvegarde est une archive, on le reimporte
        // par le meme chemin que n'importe quelle restauration.
        let (manifest, xmls) = load_and_verify(&undo).expect("archive d'annulation");
        let options = ImportOptions {
            conflict_policy: Some(ConflictPolicy::Overwrite),
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let resolver = PasswordResolver::new(None, None, false);
        let annulation = build_plan(
            &scheduler,
            &manifest,
            &xmls,
            &options,
            &resolver,
            &NullInteractor,
        )
        .unwrap();
        assert_eq!(annulation[0].action, ImportAction::Update);
        let rapport = execute_plan_with_undo(&scheduler, &annulation, false, None).unwrap();
        assert_eq!(rapport.updated, vec!["\\A".to_string()]);
        assert_eq!(
            scheduler.get_task_xml("\\A").unwrap(),
            "<Task>vieux</Task>",
            "l'annulation doit reinstaurer la definition d'origine"
        );
    }

    /// Si la sauvegarde echoue, la tache n'est **pas** ecrasee : perdre une
    /// definition faute d'avoir pu la copier serait exactement le defaut
    /// qu'on corrige.
    #[test]
    fn sauvegarde_impossible_laisse_la_tache_intacte() {
        let scheduler = scheduler_avec_tache_existante();
        let plan = plan_d_ecrasement(&scheduler);

        let tmp = tempfile::tempdir().unwrap();
        let fichier = tmp.path().join("blocage");
        std::fs::write(&fichier, b"x").unwrap();
        let dir = fichier.join("undo"); // parent = fichier : creation impossible

        let report = execute_plan_with_undo(&scheduler, &plan, false, Some(&dir)).unwrap();
        assert!(report.updated.is_empty(), "aucun ecrasement");
        assert_eq!(scheduler.registration_count(), 0, "aucune ecriture");
        assert_eq!(report.failed.len(), 1, "echecs inattendus : {:?}", report.failed);
        assert!(
            report.failed[0].1.contains("sauvegarde impossible"),
            "message inattendu : {:?}",
            report.failed
        );
        // Seule tache du plan et en echec : code 2 (echec complet).
        assert_eq!(report.exit_code(), 2);
    }

    /// Dry-run : ni tache ecrite, ni sauvegarde creee.
    #[test]
    fn dry_run_ne_cree_aucune_sauvegarde() {
        let scheduler = scheduler_avec_tache_existante();
        let plan = plan_d_ecrasement(&scheduler);

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("undo");
        let report = execute_plan_with_undo(&scheduler, &plan, true, Some(&dir)).unwrap();
        assert_eq!(report.updated, vec!["\\A".to_string()]);
        assert!(report.backed_up.is_empty());
        assert!(!dir.exists(), "aucun dossier d'annulation en dry-run");
        assert_eq!(scheduler.registration_count(), 0);
    }

    /// Une creation n'ecrase rien : aucune sauvegarde, aucun dossier cree.
    #[test]
    fn creation_ne_sauvegarde_rien() {
        let (r, xml) = record("\\A", LogonType::None, None, "<Task>a</Task>");
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let resolver = PasswordResolver::new(None, None, false);
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor)
            .unwrap();

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("undo");
        let report = execute_plan_with_undo(&scheduler, &plan, false, Some(&dir)).unwrap();
        assert_eq!(report.created.len(), 1);
        assert!(report.backed_up.is_empty());
        assert!(!dir.exists());
    }
}
