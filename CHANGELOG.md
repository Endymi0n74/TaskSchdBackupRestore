# Changelog

## 1.2.0 — 2026-10-25

### Robustesse à l'export et à la restauration

- **Plus de collision de noms de fichiers à l'export** : deux chemins de tâches distincts
  pouvaient produire le même `xml_file` (`\A__B\T` et `\A\B__T` donnaient tous deux
  `A__B__T.xml`) et l'un des deux XML écrasait l'autre. L'encodage est désormais injectif
  (`_` → `_u`, séparateur `\` ou `/` → `__`). **Les archives déjà exportées restent
  importables** : c'est le `manifest.json` qui fait foi, jamais le nom du fichier.
- **Archives refusées à la validation** quand le manifeste nomme deux fois la même tâche,
  quand deux tâches pointent vers le même fichier XML (identité résolue comme Windows la
  voit : casse, espace ou point final, flux `::$DATA`), et quand un XML n'est pas un
  document complet (un seul élément racine, rien après lui). Le message est explicite et
  aucune archive produite par l'outil n'est concernée.
- **Restauration réversible** : le XML d'une tâche sur le point d'être écrasé est copié
  dans `%LOCALAPPDATA%\TaskSchdBackupRestore\undo\<horodatage>\` avec son manifeste,
  réimportable tel quel pour revenir en arrière.
- **Élévation correctement quotée** : la ligne de commande du processus élevé échappe
  désormais les guillemets et les barres obliques finales ; un chemin comme
  `C:\Mon dossier\` n'est plus tronqué au moment de l'import élevé.


### Identité

- Projet, dépôt et application renommés **TaskSchdBackupRestore** : dossier, dépôt GitHub,
  `productName`, titre de fenêtre et identifiant `fr.endymion.taskschdbackuprestore`.
- **Deux binaires, deux noms explicites** : `TaskSchdBackupRestore.exe` (ligne de commande,
  ex-`tsbak.exe`) et `TaskSchdBackupRestoreGui.exe` (interface graphique,
  ex-`TaskBackupRestore.exe`). Le suffixe `Gui` est nécessaire : les deux vivent dans `dist/`.
- Code : le moteur vit dans `engine/` (ex-`tsbak/`), paquet `taskschdbackuprestore`,
  type d'erreur `BackupError` (ex-`TsbakError`), paquet interface `taskschdbackuprestore-gui`.
- **Données reprises** : `%LOCALAPPDATA%\TaskSchdBackupRestore` remplace
  `%LOCALAPPDATA%	sbak`. Au premier lancement, journaux et dossiers d'annulation de l'ancien
  dossier sont copiés (journaux renommés avec le nouveau préfixe), sans rien supprimer ni
  écraser — une archive d'export déjà produite reste importable telle quelle.
- Variable d'environnement de test : `TASKSCHDBACKUPRESTORE_LOG_DIR` (ex-`TSBAK_LOG_DIR`).
- Guide régénéré (`dist/Guide-TaskSchdBackupRestore.pdf`), assistants `.cmd`, README FR/EN,
  MIGRATION.md, AGENTS.md et sources du guide alignés sur la nouvelle identité.
- **Compatibilité** : seuls les scripts qui appellent le binaire changent de nom
  (`tsbak.exe` → `TaskSchdBackupRestore.exe`) ; arguments, format d'archive et journaux
  restent identiques.

## 1.1.2 — 2026-10-07

### Import — édition des actions

- **Édition des actions avant restauration** : le *Programme/script*, les *arguments* et
  le *dossier de démarrage* d'une tâche se modifient individuellement **dans le plan
  d'import**, avant toute écriture — pour re-cibler les chemins propres à la machine
  source (lettres de lecteur, partages UNC). Dans l'interface : bouton
  **« Modifier l'action… »** dans la colonne *Résolution* (formulaire aux trois champs,
  badge *Action modifiée*, « Rétablir l'action d'origine », avertissement quand la tâche
  compte plusieurs actions, désactivation s'il n'y a aucune action exécutable). En ligne
  de commande : champ `action_overrides` du fichier de réponses (mêmes clés camelCase
  pour l'interface, le CLI et le processus élevé).
- La réécriture vit dans le moteur (nouveau module `engine/src/action.rs`) et n'est
  appliquée qu'**une seule fois**, à la construction du plan : la simulation (dry-run),
  l'import réel et le processus élevé voient donc exactement le même XML, et
  **l'archive d'origine n'est jamais modifiée**. Seule la **première** action `Exec`
  d'une tâche est concernée ; les éléments manquants sont créés dans l'ordre du schéma,
  les autres champs restent strictement inchangés (un surcharge vide laisse le XML
  identique au détail près).
- Une tâche sans action `Exec` fait échouer le plan avec un message explicite, et une
  tâche dont on modifie le programme est reclassée « Mettre à jour » plutôt que
  « Identique ». Guide (section 4.2 + dépannage), README FR/EN, MIGRATION.md et
  `engine/README.md` mis à jour.

## 1.1.1 — 2026-10-06

### Fermeture

- **Le processus ne survit plus à la fermeture de la fenêtre** (bug observé
  sous Windows Server 2016 / 2019 : `TaskSchdBackupRestoreGui.exe` restait actif dans
  le gestionnaire des tâches après avoir fermé l'interface). L'application est
  mono-fenêtre : la fermeture de la fenêtre déclenche désormais une **sortie
  déterministe** du processus (`src-tauri/src/shutdown.rs`), *avant* toute
  déconnexion WebView2 — l'étape qui pouvait bloquer sur ces serveurs. La
  dernière ligne de journal est écrite avant la sortie ; les processus enfants
  WebView2 détectent la fin de l'hôte et s'arrêtent d'eux-mêmes.

### Sécurité

- **Purge des fichiers de travail orphelins de l'import élevé**
  (`%TEMP%\task-backup-restore\`) : le fichier de réponses, qui contient
  brièvement les mots de passe sur disque, est supprimé dès l'ouverture de
  l'interface (fichiers de plus d'une heure), avant chaque import élevé, et
  **à la fermeture** (tous les fichiers). Plus aucun secret abandonné si
  l'interface est fermée pendant l'invite UAC ou si une session précédente a
  planté. Tests unitaires dédiés.

### Ménage de code

- Journalisation d'import dédoublonnée : `log_import_report` (étapes +
  synthèse) et `log_import_summary` (synthèse) sont désormais partagés entre
  la commande `import_execute` et le processus helper — même lignes de
  journal des deux côtés, une seule implémentation.
- Une seule implémentation de `make_scheduler`, de `build_answers` (décisions
  → fichier de réponses, complété par les mots de passe côté helper) et de
  `local_host_name` (désormais dans le crate `TaskSchdBackupRestore`, partagé par le CLI et
  l'interface).

### Intégration continue

- Nouveau workflow [`ci.yml`](.github/workflows/ci.yml) : `cargo test` des deux
  crates (`TaskSchdBackupRestore` + `src-tauri`) à chaque *push* / *pull request* sur Windows —
  le contrôle du guide PDF restait jusqu'ici le seul job automatisé.

## 1.1.0 — 2026-09-22

### Export

- **Archive `.zip` autonome** : en mode archive, les XML et le `manifest.json` sont écrits
  dans un dossier temporaire (`%TEMP%\TaskSchdBackupRestore-export\<pid>-<nanos>`, purgé au-delà de 7 jours)
  **puis** compressés vers le dossier choisi — le dossier de destination ne reçoit plus que
  le fichier `.zip`, jamais les fichiers d'export à côté. Le nom d'archive est validé avant
  toute écriture (nom invalide = aucun fichier sur disque), une archive partielle est
  supprimée si la compression échoue, et le dossier temporaire est supprimé dans tous les
  cas. Tests unitaires `MockScheduler` dédiés (`export_archive_tests`).

### Interface

- **Thème commutable** : `ui/style.css` ne définit plus la palette (structure et composants
  seulement). Le **thème publié** vit dans `ui/themes/legacy.css` (palette historique, active
  par défaut, rendu au pixel près identique à la version précédente) et la **variante locale**
  Hestia (bleu marine / orange) dans `ui/themes/hestia.css`. `tools/select-theme.ps1` bascule
  le thème actif (`-Action Set -Theme hestia|legacy`, `-Action Status`, `-Action Check
  -Require legacy`) : l'interface étant embarquée dans l'exe au moment de la compilation,
  une compilation locale peut porter la variante, tandis que `tools/make-release.ps1`
  **refuse** de produire un livrable si la variante est active.

### Outillage

- **Guide PDF automatisé** : le workflow GitHub Actions
  [`guide-pdf.yml`](.github/workflows/guide-pdf.yml) régénère `dist/Guide-TaskSchdBackupRestore.pdf` et
  `dist/Memo-motifs.pdf` (Edge headless) et les commite dès qu'une source de `dist/guide/` change
  sur la branche par défaut ; en *pull request* le job **échoue** si le PDF commité est périmé
  (les PDF régénérés sont joints en artefact). Les empreintes des sources de chaque PDF sont
  gérées par [`tools/guide-pdf.ps1`](tools/guide-pdf.ps1) (`-Action Check` | `Build` | `Update`)
  et enregistrées dans `dist/guide/pdf-sources.sha256`.

## 1.0.0 — 2026-09-12

Première version publiée (dépôt GitHub **TaskSchdBackupRestore**). Version unifiée
`1.0.0` pour le CLI, l'interface et le manifeste Tauri.

### Export / import

- **Export** : liste des tâches planifiées, sélection par cases à cocher, filtres
  d'inclusion/exclusion (motifs `*`), export vers un dossier — XML brut + `manifest.json`
  (empreintes SHA-256).
- **Archive `.zip` restreinte au contenu d'export** : une archive ne contient que les XML de
  tâches et le `manifest.json`. Tout autre fichier présent dans le dossier d'export (ancienne
  archive, journal, note…) est **ignoré**, à la compression comme à l'extraction. Une archive
  tierce contenant des fichiers étrangers n'écrit plus ces fichiers sur disque à l'import.
- **Import** : dossier d'export **ou** archive `.zip` (détection automatique, protection
  zip-slip), plan résolu tâche par tâche (créer / mettre à jour / ignorer / conflit /
  utilisateur non mappé / mot de passe requis), **simulation (dry-run)** strictement identique
  à l'exécution réelle, puis import réel.
- **Mot de passe unique** : un champ masqué « Mot de passe unique » + bouton « Remplir avec le
  même mot de passe » applique un seul secret à toutes les tâches « Mot de passe requis » du
  plan (guide, sous-section 4.1).
- **Élévation à la volée** : l'import réel s'exécute dans un processus enfant élevé temporaire
  (invite UAC), l'interface reste ouverte et affiche le rapport à la fin.

### Ligne de commande

- `TaskSchdBackupRestore.exe` — même moteur que l'interface, **Windows Server 2008 R2 → 2025+**, aucune
  dépendance runtime (COM Task Scheduler présent depuis Vista).
- `--hide-microsoft` sur `TaskSchdBackupRestore list` et `TaskSchdBackupRestore export` : exclut les tâches système
  `\Microsoft\`, comme l'interface où elles sont masquées par défaut.

### Interface

- Onglet *Tâches & Export* : les tâches `\Microsoft\` sont **masquées par défaut** (case à
  cocher pour les afficher) ; le compteur et la barre de statut indiquent le nombre de tâches
  masquées.
- Journalisation partagée avec le CLI (`%LOCALAPPDATA%\TaskSchdBackupRestore\logs\`, rétention 14 jours),
  consultable en direct dans l'onglet *Logs*.

### Sécurité

- Les mots de passe (comptes de tâches, archives) ne sont **jamais journalisés** ni écrits sur
  disque par l'interface : ils restent en mémoire dans le backend et sont transmis au processus
  élevé par un fichier de réponses JSON temporaire (ACL utilisateur, supprimé dans tous les cas).
- Manifeste + empreintes SHA-256 + XML bien formés vérifiés **avant** toute écriture ; chemins
  non sûrs (zip-slip) et entrées étrangères refusés.

### Documentation et livraison

- Guide illustré [`dist/Guide-TaskSchdBackupRestore.pdf`](dist/Guide-TaskSchdBackupRestore.pdf) et mémo d'une page
  [`dist/Memo-motifs.pdf`](dist/Memo-motifs.pdf) (filtres d'inclusion/exclusion), sources dans
  `dist/guide/`.
- [`MIGRATION.md`](MIGRATION.md) — procédure pas à pas 2008 R2 → 2022.
- Assistants `.cmd` (`export.cmd`, `import.cmd`, `validate.cmd`) pour les serveurs sans
  interface ; [`tools/make-release.ps1`](tools/make-release.ps1) assemble le dossier de
  livraison et l'archive de release.
- Nettoyage de code : suppression de fonctions mortes (`manifest_summary`, `run_helper`).
