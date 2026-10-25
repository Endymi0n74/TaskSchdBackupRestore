# memory.md — TaskSchdBackupRestore

Mémoire du projet : historique, décisions structurantes, leçons apprises. À lire
en complément de [`README.md`](README.md) et [`AGENTS.md`](AGENTS.md).

## En bref

- **Statut** : rouvert le 2026-10-25 — revue de code et renommage d'identité, travail
  **non commité** sur `fix/revue-sauvegarde` ; dernier état **publié** : **1.1.2** sur `master`,
  **1.2.0** construit et assemblé localement (non publié). Sujet **sans rapport** avec
  `SteamAchievementManager` (SAM, `D:\Codex\SteamAchievementManager`, clos lui aussi le
  même jour) ni avec l'outillage **REA** (`D:\Codex\tools\memory.md`) : trois entrées
  indépendantes, ne pas les mélanger. **Ne pas rouvrir sans demande explicite.**
- **Quoi** : export/import des tâches planifiées Windows (Task Scheduler) en XML brut.
- **Chemin** : `D:\Codex\TaskSchdBackupRestore` (dépôt Git autonome, branche `master`) — publié sur
  GitHub sous **TaskSchdBackupRestore** (`Endymi0n74/TaskSchdBackupRestore`).
- **Deux interfaces, un moteur** : `TaskSchdBackupRestore` (crate Rust + CLI) et `TaskSchdBackupRestoreGui.exe`
  (Tauri 2). Livraison portable dans `dist/`.
- **Dernière version** : **1.2.0** (2026-10-25, non publiée) — renommage d'identité et
  correctifs de sûreté de données (collision de noms à l'export, archives refusées à la
  validation, écrasement réversible). Avant elle : **1.1.2** (2026-10-07) — édition des
  actions avant restauration ; **1.1.1** (2026-10-06) — sortie déterministe à la fermeture (processus
  fantôme sur Server 2016/2019) + purge des fichiers de travail secret ; **1.1.0**
  (2026-09-22) — thème commutable + correctif d'export en
  archive, et **1.0.0** (2026-09-12), première version publiée ; les numérotations
  intermédiaires (1.1.0 du 2026-09-10) ont été **renumérotées en 1.0.0** lors de la création du
  dépôt (les entrées de dates ci-dessous gardent leur numérotation d'origine — l'entrée
  « 2026-09-10 » parle donc d'une **ancienne** 1.1.0 abandonnée, sans rapport avec celle de
  2026-09-22).

## Renommage d'identité — 2026-10-25

Le projet, le dépôt et l'application s'appellent désormais **TaskSchdBackupRestore** partout :
dossier `D:\Codex\TaskSchdBackupRestore`, dépôt `Endymi0n74/TaskSchdBackupRestore`, `productName`,
titre de fenêtre et identifiant `fr.endymion.taskschdbackuprestore`.

- **Deux binaires, deux noms** : `TaskSchdBackupRestore.exe` (CLI, ex-`tsbak.exe`) et
  `TaskSchdBackupRestoreGui.exe` (interface, ex-`TaskBackupRestore.exe`). Le suffixe `Gui` est
  nécessaire : les deux vivent dans `dist/` et ne peuvent pas porter le même nom.
- **Code** : crate du moteur dans `engine/` (ex-`tsbak/`), paquet `taskschdbackuprestore`,
  type d'erreur `BackupError` (ex-`TsbakError`), paquet interface `taskschdbackuprestore-gui`
  dont le binaire est déclaré `[[bin]]` pour garder son nom d'affichage.
- **Données reprises** : `engine::data_dir()` est le seul propriétaire du chemin
  (`%LOCALAPPDATA%\TaskSchdBackupRestore`). Au premier appel il **copie** les journaux et dossiers
  d'annulation de `%LOCALAPPDATA%	sbak` (journaux renommés avec le nouveau préfixe, aucun
  fichier écrasé) ; test `reprise_des_donnees_de_l_ancien_dossier`. L'interface et le CLI
  dérivent désormais leur dossier de là, plus aucune duplication du chemin.
- **Variable de test** : `TASKSCHDBACKUPRESTORE_LOG_DIR` (ex-`TSBAK_LOG_DIR`).
- **Livraison** : `dist/` contient les deux exécutables reconstruits, `dist/Guide-TaskSchdBackupRestore.pdf`
  (régénéré par Edge, empreintes `pdf-sources.sha256` à jour).
- **Renommer le dossier casse le cache de build de l'interface** : `src-tauri/target` contient
  des chemins absolus (`build/tauri-*/out/permissions/...` du script de build Tauri) ;
  après le `mv` du dossier, `cargo test` échoue sur « failed to read plugin permissions »
  pointant l'**ancien** chemin. `cargo clean` dans `src-tauri` résout, aucun code à changer.
- **Leçon** : les entrées de ce fichier antérieures à cette date sont écrites avec le **nom
  actuel** ; les artefacts réellement produits à l'époque (archives de `release/`, vieux `.zip`)
  portent l'ancien nom et restent reconnaissables à leur contenu, pas à leur titre.

## Dates clés

- **2026-09-08** — Premier test réel de l'implémentation COM sur machine Windows :
  trois bugs réels trouvés et corrigés (voir « Leçons apprises »).
- **2026-09-09** — Migration Tauri 1 → Tauri 2 terminée (dépôt consolidé) ; guide
  illustré PDF produit (captures réelles de l'interface, de la console et des
  assistants `.cmd`).
- **2026-09-09/10** — v1.0.0 : archives `.zip` restreintes (XML + manifeste uniquement),
  tâches Microsoft masquées par défaut, nettoyage de code, version unifiée.
- **2026-09-10** — v1.1.0 : option CLI `--hide-microsoft` (list/export) ; `MIGRATION.md`
  (procédure 2008 R2 → 2022) ; section dédiée « filtres Inclure/Exclure » dans le guide
  (section 3) ; mémo d'une page `dist/Memo-motifs.pdf` ; script `make-pdf.ps1`
  (Edge headless) régénère les PDF depuis les sources HTML modifiables.
- **2026-09-11** — Guide : nouvelle sous-section **4.1 « Appliquer un mot de passe unique à
  toutes les tâches »** (champ « Mot de passe unique » + bouton « Remplir avec le même mot de
  passe », étape 5 de la section 4, note CLI `--password-file`, ligne de dépannage),
  vignette annotée `dist/guide/shots/10-import-password.png`, figures renumérotées 1→10.
  La vignette a été capturée sur un **build release local**
  (`src-tauri/target/release/TaskSchdBackupRestoreGui.exe`) ; les binaires de `dist/` ont ensuite
  été reconstruits.
- **2026-09-11 (suite)** — **`dist/TaskSchdBackupRestoreGui.exe` reconstruit** depuis l'arbre courant
  (nouveau SHA-256 `f03dda61…`) : l'interface embarque bien la sous-section 4.1 (champ
  « Mot de passe unique » + bouton « Remplir avec le même mot de passe »), vérifié dans le
  cache d'assets brotli du build. `dist/TaskSchdBackupRestore.exe` **inchangé** — identique au binaire
  recompilé, déjà `1.1.0` avec `--hide-microsoft` (la fonctionnalité est purement côté
  interface). Restent à rafraîchir avant publication : `release/TaskSchdBackupRestore-1.1.0-windows/` et son
  `.zip`, qui datent du 2026-09-10 et contiennent encore l'ancienne interface.
- **2026-09-12** — **Artefacts de release reconstruits** depuis `dist/` par un script
  désormais versionné, `tools/make-release.ps1` (copie sélective, exclusion des fichiers de
  travail et des données d'export de la machine, puis archive `.zip` à entrées `/`).
  `Memo-motifs.pdf` est **ajouté** à la livraison (il manquait alors que le `guide/` embarqué
  contient déjà `memo-motifs.html`) et `LISEZMOI.txt` le mentionne.
- **2026-09-12 (publication)** — **Rebranding en 1.0.0** : version unifiée `1.0.0`
  (`engine/Cargo.toml`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`), produit
  renommé **TaskSchdBackupRestore**, **les deux binaires recompilés** (`TaskSchdBackupRestore.exe`
  `6730a7ad…` — `TaskSchdBackupRestore --version` répond bien `1.0.0` ; `TaskSchdBackupRestoreGui.exe` `ea9b0750…`,
  FileVersion `1.0.0`, chaîne Rust `import_set_password` présente dans le binaire), CHANGELOG
  consolidé en une seule entrée `1.0.0 — 2026-09-12`, mentions `v1.1.0` neutralisées dans
  `MIGRATION.md` et les sources du guide. Livraison `release/TaskSchdBackupRestore-1.0.0-windows/`
  (45 fichiers) + `TaskSchdBackupRestore-1.0.0-windows.zip` SHA-256 `9e8213aa…` (extraction vérifiée :
  empreintes identiques à `dist/`). Publication : dépôt GitHub créé, `v1.0.0` poussé et
  release avec l'archive — voir « État actuel ».
- **2026-09-12 (CI)** — **Guide PDF automatisé** : `tools/guide-pdf.ps1`
  (`-Action Check` | `Build` | `Update`) enregistre l'empreinte des sources de chaque PDF dans
  `dist/guide/pdf-sources.sha256` (guide : `guide.html` + `shots/` + `make-pdf.ps1` ; mémo :
  `memo-motifs.html` + `shots/` + `make-pdf.ps1`) et le workflow
  `.github/workflows/guide-pdf.yml` régénère + commite les PDF quand une source change
  (push sur `master`, `[skip ci]`) mais **échoue** en pull request si le PDF commité est
  périmé. Les sources sont relues par `git ls-files` (les fichiers non suivis sont signalés)
  et les fins de ligne sont normalisées en LF avant hachage (`core.autocrlf=true` ici, aucun
  `.gitattributes`) : mêmes empreintes en local et sur le runner. Vérifié le 2026-09-12 : les
  deux PDF commités sont **identiques en contenu** à un build neuf (guide : 12 octets de
  métadonnées seulement ; mémo : flux de contenu identiques, seuls version Edge 153→154 et
  horodatage changent), donc aucune retouche manuelle n'a été écrasée.
- **2026-09-22 (release 1.1.0)** — **Correctif d'export en archive** : en mode archive, les XML
  et le `manifest.json` sont désormais écrits dans un dossier temporaire
  (`%TEMP%\TaskSchdBackupRestore-export\<pid>-<nanos>`, purgé au-delà de 7 jours) **puis** compressés vers le
  dossier choisi — le dossier de destination ne reçoit **plus que le `.zip`** (avant : les
  fichiers d'export étaient déposés à côté de l'archive). Nom d'archive validé avant toute
  écriture, archive partielle supprimée si la compression échoue, staging supprimé dans tous
  les cas. Découpage `export_tasks_impl` → `export_with` → `write_export`, partagé avec les
  tests e2e (`export_tasks_impl` conservée, signature inchangée) ; tests unitaires
  `MockScheduler` dans `export_archive_tests` (destination = zip seul, export simple inchangé,
  nom invalide sans écriture, staging toujours supprimé). **Thème commutable** livré
  (`ui/themes/legacy.css` publié, `hestia.css` variante, `select-theme.ps1`), `make-release.ps1`
  refuse de livrer une variante active. Version unifiée **1.1.0** dans les trois fichiers
  canoniques.
- **2026-10-06 (release 1.1.1)** — **Sortie déterministe à la fermeture** : sous Windows
  Server 2016/2019, fermer l'interface laissait `TaskSchdBackupRestoreGui.exe` actif dans les
  processus (la déconnexion WebView2 / la fin de boucle d'événements pouvait bloquer sur
  ces serveurs). L'application étant mono-fenêtre, `on_window_event` intercepte
  maintenant `CloseRequested` et appelle `shutdown::exit_on_window_close` (nouveau module
  `src-tauri/src/shutdown.rs`) : purge des fichiers de travail, dernière ligne de
  journal, `std::process::exit(0)` — **avant** toute déconnexion d'objets. Vérifié sur
  Win11 (binaire 1.1.0 → fermeture identique, aucun enfant WebView2 résiduel) ; la
  détermination rend l'ancien biais 2016/2019 impossible par construction.
  **Purge des secrets transitoires** : les fichiers de `%TEMP%\TaskSchdBackupRestore\`
  (`answers-*` avec mots de passe, `result-*`) sont purgés à l'ouverture (> 1 h), avant
  chaque import élevé et **en totalité à la fermeture** — plus aucun secret abandonné si
  l'interface fermait pendant l'UAC. **Ménage** : journalisation d'import dédoublonnée
  (`log_import_report`/`log_import_summary` partagés helper ↔ commande), une seule
  implémentation de `make_scheduler` / `build_answers` / `local_host_name` (ce dernier
  monté dans le crate `TaskSchdBackupRestore`), tests unitaires de la purge. **CI** : nouveau workflow
  `ci.yml` (`cargo test` des deux crates sur `windows-latest` à chaque push/PR —
  auparavant seul le guide PDF était contrôlé), `actions/checkout` passé à `v5`.
  Version unifiée **1.1.1** dans les trois fichiers canoniques. **Publication** :
  `v1.1.1` poussé + release GitHub `TaskSchdBackupRestore-1.1.1-windows.zip` (46 fichiers),
  empreintes `TaskSchdBackupRestoreGui.exe` `15a05d62…`, `TaskSchdBackupRestore.exe` `5f1ee00d…`,
  zip `bfe47186…` ; les deux étapes de la CI sont vertes. Premier run en échec :
  test `nom_archive_invalide_ne_cree_rien` faussé par l'export concurrent d'un
  autre test (cf. leçon ci-dessous) — corrigé par un verrou partagé (`a3fc189`).
- **2026-10-07 (édition des actions avant import)** — Nouvelle fonctionnalité : permettre de
  changer individuellement, avant restauration, les champs d'action d'une tâche
  (*Programme/script*, *Ajouter des arguments*, *Commencer dans*) car les chemins diffèrent
  d'une machine à l'autre. Moteur `engine/src/action.rs` : `read_action` (lecture des valeurs
  en vigueur + nombre d'actions pour pré-remplir l'interface) et `apply_action` (réécriture de
  la première action `Exec`, éléments manquants créés dans l'ordre du schema, échappement XML
  conservé, reste du document intact ; erreur claire si aucune action `Exec`). Appliqué dans
  `build_plan` **après** le saut explicite et **avant** la comparaison d'existant : une tâche
  dont on change le programme passe d'« Identique » à « Mettre à jour ». Transport par
  `AnswerFile.action_overrides` → décisions GUI (`ImportDecisions`), answer-file CLI et
  fichier de réponses du helper élevé partagent la même structure. Interface : bouton
  « Modifier l'action… » dans la colonne *Résolution* (formulaire inline à trois champs,
  badge *Action modifiée*, « Rétablir l'action d'origine », avertissement multi-actions,
  bouton sans objet si aucune action `Exec`). Docs : guide **4.2** + ligne de dépannage +
  note answer-file (section 8), README FR/EN, MIGRATION.md, `engine/README.md` — PDF
  régénéré via `tools/guide-pdf.ps1 -Action Build`. Tests : 16 unitaires dans `action.rs`
  (indentation, insertion ordonnée, échappement, multi-Exec, préfixe XML, auto-fermant,
  sérialisation), 4 dans `import.rs` (écriture, reclassification, identité, erreur) et 2
  côté `src-tauri` (roundtrip answer-file, import élevé).

- **2026-10-07 (release 1.1.2)** — **Publication de l'édition des actions** : version
  unifiée **1.1.2** dans les trois fichiers canoniques (`Cargo.lock` synchronisés pour la
  CI `--locked`), entrée CHANGELOG 1.1.2, guide remis à jour (chip « 🆕 Version 1.1.2 »,
  pied de page) + mémo, README FR/EN et AGENTS alignés, PDF régénérés et empreintes de
  sources mises à jour. Binaires release reconstruits (CLI : `TaskSchdBackupRestore 1.1.2` confirmé ;
  interface : `FileVersion` 1.1.2 avec l'UI embarquée du bouton d'édition) et recopiés
  dans `dist/` ; livrable assemblé par `tools/make-release.ps1` (dossier +
  `TaskSchdBackupRestore-1.1.2-windows.zip`, 49 fichiers, empreintes : interface `ba350230…`, CLI
  `a58a7746…`, zip `ccd88c1e…`). **Publication** : `v1.1.2` poussé (commit release
  `6bf5b73`, fonctionnalité `f4b70cc`) + release GitHub « TaskSchdBackupRestore 1.1.2 »
  avec le zip (4 859 220 octets) ; CI et Guide PDF verts sur le commit de release.

- **2026-10-25 (revue, 6 correctifs + écrasement réversible, non publiés)** — Revue du code
  puis correctifs confirmés par exécution, sur `fix/revue-sauvegarde` (**rien de commité**,
  version 1.1.2) :
  1. **Collision de noms à l'export** (`export.rs`) : `\A__B\T` et `\A\B__T` donnaient le même
     `A__B__T.xml` ; un seul XML était écrit et l'archive **entière** était ensuite refusée
     (empreinte), perte silencieuse vue seulement à la restauration. `sanitize_filename` est
     **injectif** (`\` → `__`, `_` → `_u`). **À signaler** : un chemin contenant `_` change de
     nom (`\A_B` → `A_uB.xml`) ; les archives existantes restent importables, leur
     `manifest.json` enregistrant le nom réel.
  2. **Manifeste non validé** (`import.rs`) : `xml_file` hors du dossier (`C:\Windows\win.ini`,
     `..\..\secret.xml`) lisait n'importe quel fichier — tout composant non `Normal` est refusé,
     les sous-dossiers internes (`sous/A.xml`) restent acceptés.
  3. **« XML bien formé » jamais vérifié** : `read_event()` ne lisait qu'un événement. Exige un
     **document complet** (une racine, hors racine seulement espaces/commentaires/PI, rien
     après ; DOCTYPE refusé où qu'il soit, y compris dans la racine). Les 173 tâches réelles de
     la machine valident.
  4. **`quote_arg` cassait la ligne de commande de l'import élevé** (`src-tauri/src/helper.rs`) :
     `"...\"` pour un chemin finissant par `\` faisait déraper le découpage (`--answers` avalé,
     chemin tronqué), et un guillemet du chemin était supprimé. Règles de `CommandLineToArgvW`
     appliquées ; ligne construite par `elevated_params` (publique, pour les tests).
  5. **Chemin de tâche dupliqué** et **6. `xml_file` partagé par deux tâches** (`load_and_verify`) :
     les deux étaient acceptés (code 0) et le plan sortait deux `CREATE` sur la même tâche — une
     définition restaurée sous deux noms, état partiel si le second enregistrement échouait.
     Refusés au chargement (donc dans `validate`, `import` et l'interface), messages nommant le
     chemin, le fichier et les deux tâches.
     **Leçon** : une première version normalisait la chaîne à la main (minuscules, `/` → `\`) ;
     l'audit a montré par le vrai binaire que `A.xml `, `A.xml.` et `A.xml::$DATA` passaient
     encore sur un dossier qui ne contient qu'un `A.xml`. Deux règles les attrapent désormais,
     dans `resolve_xml_file` : (a) **une seule règle de nom** — un composant ne doit pas finir
     par un espace ou un point, ni contenir `:` (flux NTFS), un caractère interdit (`? * < > " |`)
     ou de contrôle, refusé **avant tout accès disque** ; (b) la clé de doublon est le **chemin
     canonique** (`fs::canonicalize`), qui résout casse et séparateurs — identité de chemin, pas
     d'objet : deux liens physiques vers le même contenu resteraient distincts, et le fichier
     doit exister (validate l'exigeait déjà ; son contenu n'est pas lu à ce stade).
  7. **Écrasement réversible** (`execute_plan_with_undo`) : la seule perte irréversible restante.
     Avant de remplacer une tâche existante, son XML **actuel** est copié dans
     `%LOCALAPPDATA%\TaskSchdBackupRestore\undo\<horodatage>` (ou `--undo-dir`), avec un `manifest.json` : le
     dossier est une **archive réimportable** (`TaskSchdBackupRestore import <dossier>`). Si la copie échoue,
     la tâche n'est **pas** écrasée (échec rapporté, code 2 si c'était la seule tâche). Câblé
     dans le CLI, l'interface et le helper élevé (l'élévation UAC garde le même profil) ; le
     dry-run n'écrit ni tâche ni sauvegarde. `annulation_reinstalle_le_xml_ecrase` prouve le
     cycle complet : écrasement -> réimport du dossier d'annulation -> la tâche retrouve son
     XML d'origine. Pas d'écriture réelle dans le planificateur (l'import exige l'élévation) :
     la preuve est au niveau moteur (`MockScheduler` + disque réel), et le dossier produit est
     accepté par le vrai binaire (`TaskSchdBackupRestore validate` puis `import --dry-run`).
  7 tests de régression écrits **avant** les correctifs (échouent sans, passent avec) ; 72 + 4
  tests `TaskSchdBackupRestore`, 26 + 1 `src-tauri` verts, clippy inchangé (3 remarques préexistantes côté
  `TaskSchdBackupRestore`, 0 côté interface). Preuves réelles : seam zip (`zip_dir` → `unzip_verified` →
  `load_and_verify`, 5 noms piégeux, échoue si l'encodage régresse), archives fabriquées refusées
  une par une en CLI, export de 173 tâches revalidé après chaque correctif, ligne de l'import
  élevé relue par `CommandLineToArgvW` **et** par un lancement réel, e2e `#[ignore]` rejoué.

- **2026-10-25 (release 1.2.0, non publiée)** — Version unifiée **1.2.0** dans les trois
  fichiers canoniques ; CHANGELOG (entrée 1.2.0 : identité + robustesse), guide (chip et
  pied de page), README FR/EN, AGENTS.md §7 alignés, PDF régénérés (`-Action Build`, empreintes
  enregistrées). Binaires reconstruits (`TaskSchdBackupRestore 1.2.0`, `FileVersion` 1.2.0)
  recopiés dans `dist/`, livrable assemblé par `tools/make-release.ps1` :
  `release/TaskSchdBackupRestore-1.2.0-windows/` + `.zip`, 49 fichiers (CLI `3ed987d4…`,
  interface `a8504646…`, zip `e1b51380…`), notes dans `release/RELEASE-NOTES-1.2.0.md`.
  Tests 73 + 4 et 26 + 1 verts. **Rien n'est poussé** : commit, tag `v1.2.0` et release
  GitHub restent à faire.

## Décisions structurantes

1. **Le CLI est le moteur** : `TaskSchdBackupRestore.exe` fonctionne sur **toutes** les versions de
   Server 2008 R2 → 2025+ (aucune dépendance runtime, COM présent depuis Vista).
   L'interface (WebView2) est limitée à Win10/11 et Server 2016+ — Microsoft ne
   patchant plus WebView2 sur 2008 R2/2012 (figé en 109 depuis oct. 2023), c'est un
   choix de sécurité assumé, documenté dans le README et le guide.
2. **Classification unique** : `build_plan` calcule une seule fois la décision
   (créer/mettre à jour/sauter/conflit/mot de passe requis/utilisateur non mappé) ;
   `execute_plan(dry_run)` ne fait que sauter l'écriture. Dry-run et réel donnent donc
   un rapport strictement identique.
3. **Secrets jamais journalisés** : mots de passe en mémoire dans l'état backend,
   transmis au processus élevé via un fichier JSON temporaire (ACL utilisateur,
   supprimé dans tous les chemins). Aucun log, aucune vue ne transporte un secret.
4. **Élévation à la volée** : l'import réel non élevé lance le même exe avec
   `--helper-import` via `ShellExecuteExW runas` (UAC) — l'interface reste ouverte.
5. **ZIP = contenu d'export strict** : seuls les XML de tâches + `manifest.json` entrent
   dans une archive (et seuls ces fichiers en sortent à l'extraction). Les fichiers
   étrangers sont ignorés — cf. `is_export_entry` dans `src-tauri/src/archive.rs`.
6. **Interface : tâches Microsoft masquées par défaut** (`\Microsoft\*`), case à cocher
   « Masquer les tâches Microsoft » dans `ui/app.js`. Décision prise le 2026-09-09/10
   pour réduire le bruit système (des centaines de tâches Windows dans la liste).
7. **Résolution des conflits** : politique `--conflict-policy`/fichier de réponses ;
   pas de mapping utilisateur pour les comptes bien connus (SYSTEM, LOCAL SERVICE…).
   Les tâches `TASK_LOGON_PASSWORD` sont validées par Windows lui-même au
   `RegisterTask` (HRESULT traduit) ; le mapping explicite n'est requis que pour S4U /
   jetons interactifs.
8. **Guide PDF : contrôle par empreinte des sources, jamais par comparaison binaire.** Edge
   headless n'est pas reproductible (version, horodatage, `/ID`) et le PDF peut être retouché à
   la main : comparer les octets produirait un échec permanent. `tools/guide-pdf.ps1` enregistre
   donc l'empreinte des sources de chaque PDF (`dist/guide/pdf-sources.sha256`) ; le workflow
   régénère et commite les PDF sur la branche par défaut mais échoue en pull request si le PDF
   commité est périmé. `-Action Update` déclare les sources couvertes **sans** régénérer, pour
   un PDF livré retouché à la main.
9. **Édition des actions avant import** : les champs d'action d'une tâche — *Programme/script*
   (`<Command>`), *Ajouter des arguments* (`<Arguments>`), *Commencer dans*
   (`<WorkingDirectory>`) — se modifient tâche par tâche **dans le plan**, avant restauration,
   pour re-cibler les chemins propres à la machine source (lettres de lecteur, partages UNC).
   La réécriture vit dans `engine/src/action.rs` (parcours d'événements quick-xml, **première**
   action `Exec` uniquement, éléments absents créés dans l'ordre du schema, reste du document
   recopié octet à octet) et n'est appliquée qu'**une seule fois**, dans `build_plan` avant
   toute comparaison : classification, dry-run, import réel et helper élevé voient donc le
   même XML modifié, et l'archive d'origine n'est jamais réécrite. Le transport est unique :
   `action_overrides` (clés camelCase, champs optionnels) porte la même structure dans les
   décisions de l'interface, le fichier de réponses CLI et le fichier de réponses de
   l'élévation.

## Leçons apprises

- **Tests parallèles et état global partagé** : comparer un dossier temporaire
  global (`%TEMP%\TaskSchdBackupRestore-export`) avant/après dans deux tests `#[test]` est faux
  dès que les tests tournent en parallèle dans le même processus (les suffixes
  `pid-nanos` ne distinguent pas les créateurs — observé en CI, 24/25). Un
  `static STAGING_LOCK: Mutex<()>` partagé par les tests observateurs suffit
  (`.lock().unwrap_or_else(|e| e.into_inner())` pour survivre au poison).
- **`windows` 0.58 : `ShellExecuteExW` / `SHELLEXECUTEINFOW` exigent la feature
  `Win32_System_Registry`** (la structure porte un champ `hkeyClass: HKEY`) : la
  retirer casse la compilation de `helper.rs` **sans aucun appel direct à la
  registry**. Ne pas juger une feature `windows` inutile d'après les appels du code.
- **COM `windows-rs` 0.58** : `IPrincipal::UserId` et `IPrincipal::LogonType` utilisent
  des paramètres de sortie (`*mut BSTR` / `*mut TASK_LOGON_TYPE`), pas des retours directs.
- **`CoUninitialize` avant libération COM** → STATUS_ACCESS_VIOLATION à la fermeture :
  `WindowsScheduler` libère `ITaskService` dans `Drop` **avant** `CoUninitialize`
  (champ `Option` + `take()`).
- **Collections COM 1-based** : `get_Item`/`Count` démarrent à 1.
- **`CreateFolder`** échoue si le dossier existe : cas ignoré dans `ensure_folder`.
- **WebView2 figé sur anciens serveurs** : ne pas promettre l'interface au-delà de
  Server 2016 — pointer vers le CLI/assistants `.cmd`.
- **`make-pdf.ps1`** : `-Pdf` doit être un chemin **relatif** (le script fait
  `Join-Path (Get-Location) $Pdf` puis `GetFullPath`) — un chemin absolu échoue avec
  « Le format du chemin d'accès donné n'est pas pris en charge ».
- **`tools/make-release.ps1`** : sous Git Bash, `powershell -NoProfile -File <script>`
  peut rester bloqué (aucune instruction exécutée, le script n'écrit rien) alors que le même
  script lancé en `-Command "& '.\tools\make-release.ps1'"` s'exécute en ~1 s. Lancer les
  scripts `.ps1` avec la forme `-Command` dans cet environnement ; le script lui-même est
  correct (vérifié, sortie identique d'une exécution à l'autre).
- **Captures d'écran du guide** : les scripts de capture (`dist/guide/scripts/*.ps1`)
  existent et ont servi à produire les vignettes ; la capture des assistants `.cmd`
  interactifs est fragile (positionnement fenêtre console, timing). L'utilisateur
  retouche ces vignettes à la main.

## État actuel / à faire

- [x] Export/import XML + manifeste SHA-256, filtres, dry-run, ZIP AES-256.
- [x] Élévation à la volée (helper), logs partagés CLI/GUI.
- [x] ZIP restreint (XML + manifeste uniquement), extraction filtrée.
- [x] Tâches Microsoft masquées par défaut.
- [x] v1.0.0 : version unifiée, CHANGELOG, README GitHub.
- [ ] Guide PDF : vignettes 8-11 à refaire à la main par l'utilisateur (ne pas toucher).
      Attention à la renumérotation du 2026-09-11 : les figures ≥ 4 sont décalées de +1
      (ancienne figure 8 = figure 9, etc.).
- [x] Guide PDF : régénération automatisée (2026-09-12) — `tools/guide-pdf.ps1`
      (`Check`/`Build`/`Update`) + `.github/workflows/guide-pdf.yml` (push → régénère et
      commite, pull request → échoue si le PDF commité est périmé). Après une retouche manuelle
      du PDF, relancer `-Action Update` pour réaligner l'empreinte des sources.
- [x] Artefacts de release reconstruits (`tools/make-release.ps1`, 2026-09-12).
- [x] Rebranding **1.0.0** + dépôt GitHub **TaskSchdBackupRestore** (`v1.0.0`, release avec
      `TaskSchdBackupRestore-1.0.0-windows.zip`).
- [x] Guide PDF : le PDF livré a été régénéré (commit « regenerer le guide PDF ») puis contrôlé
      le 2026-09-12 — contenu identique à un build neuf, empreintes de référence enregistrées
      dans `dist/guide/pdf-sources.sha256`.
- [x] **Release 1.1.1** publiée le 2026-10-06 (`v1.1.1` + `TaskSchdBackupRestore-1.1.1-windows.zip`) : fermeture
      déterministe (bug Server 2016/2019), purge des secrets transitoires, ménage, CI `cargo test`
      verte (y compris après correction du test de staging).
- [x] **Release 1.1.2** publiée le 2026-10-07 (`v1.1.2` + `TaskSchdBackupRestore-1.1.2-windows.zip`,
      49 fichiers) : édition des actions avant restauration (moteur `action.rs`, bouton
      « Modifier l'action… », `action_overrides`), CHANGELOG/guides/README alignés,
      binaires reconstruits, CI verte.
- [ ] **`cargo fmt`** : le dépôt n'a jamais été formaté (diffs rustfmt massifs partout) — à faire
      dans un commit dédié, sans mélanger avec d'autres sujets, puis ajouter `cargo fmt --check`
      à `ci.yml` (ne pas l'activer avant). `clippy` est déjà propre hors remarques de style.
- [ ] Vérifier la fermeture sur un **vrai** Windows Server 2016/2019 : non reproductible sur
      Win11 (le binaire 1.1.0 y fermait déjà proprement) ; le correctif est déterministe par
      construction (`process::exit` avant toute déconnexion).
- [x] **Édition des actions avant import** (2026-10-07) : `engine/src/action.rs`
      (`read_action`/`apply_action`, première action `Exec`), surcharges transportées par
      `action_overrides` (décisions GUI → answer-file CLI → helper élevé), bouton
      « Modifier l'action… » dans le plan de l'interface, guide 4.2 + README/MIGRATION.
- [x] **Release 1.2.0** construite le 2026-10-25 (`TaskSchdBackupRestore-1.2.0-windows.zip`,
      49 fichiers) : identité + correctifs de sûreté. **Non publiée** : commit, tag `v1.2.0`,
      release GitHub et push à faire.
