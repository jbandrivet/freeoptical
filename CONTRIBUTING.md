# Contribuer à FreeOptical

Merci de vouloir améliorer FreeOptical ! Toute aide est bienvenue : signaler un bug, proposer une idée, corriger un calcul, ajouter un verre au catalogue, améliorer l'interface ou la documentation. Les échanges peuvent se faire en français ou en anglais.

## Signaler un bug

Ouvrez une [issue « Bug »](https://github.com/jbandrivet/freeoptical/issues/new/choose) en précisant :
- la version (`freeoptical --version`) et la distribution Linux ;
- ce que vous avez fait, ce que vous attendiez et ce que vous avez obtenu ;
- **le fichier `.json` du système optique** concerné (menu `Fichier` → `Sauvegarder`), c'est ce qui permet de reproduire le problème ;
- pour une erreur de calcul, la valeur attendue et sa source (calcul à la main, autre logiciel, livre…).

## Proposer une idée

Ouvrez une [issue « Idée »](https://github.com/jbandrivet/freeoptical/issues/new/choose), ou lancez une [discussion](https://github.com/jbandrivet/freeoptical/discussions) si l'idée mérite d'abord d'être débattue.

## Contribuer du code

1. Forkez le dépôt et créez une branche depuis `main` (`git checkout -b correction-pupille`).
2. Compilez et testez :
   ```bash
   cargo build --release
   cargo test
   ```
   Il faut Rust 1.85 ou plus récent (édition 2024).
3. Ouvrez une pull request vers `main` en expliquant ce qui change et pourquoi. La compilation et les tests sont relancés automatiquement sur chaque pull request.

### Organisation du code

| Dossier | Rôle |
| --- | --- |
| `src/core/` | Moteur optique : types (`types.rs`), verres (`glass.rs`), paraxial (`paraxial.rs`), lancer de rayons (`raytracer.rs`), analyses (`analysis.rs`), optimiseur DLS (`optimizer.rs`), exemples (`presets.rs`) |
| `src/gui/` | Interface graphique (egui) |
| `src/ui/` | Interface terminal (`--tui`, ratatui) |
| `src/mcp/` | Serveur MCP (`--mcp`) |
| `src/maj.rs` | Mise à jour en ligne depuis les releases GitHub |
| `packaging/` | Construction du paquet `.deb` (`make-deb.sh`) |

### Règles

- **Les calculs optiques doivent être vérifiés par un test.** Toute correction ou nouvelle fonction du moteur s'accompagne d'un test `cargo test` qui compare le résultat à une valeur connue (formule analytique, cas d'école, autre logiciel). Les exemples de `src/core/presets.rs` servent de tests de non-régression : ils doivent rester au point.
- Respectez les conventions décrites dans le [README](README.md#conventions-optiques) : épaisseurs négatives après un miroir (convention Zemax), diaphragme `STOP`, focale signée.
- Le code suit le style existant : noms et commentaires en français, pas de nouvelle dépendance sans raison.
- Une pull request = un sujet. Les petites pull requests sont relues plus vite.

## Licence

FreeOptical est sous double licence MIT ou Apache-2.0. Sauf mention contraire de votre part, toute contribution proposée pour être intégrée au projet est placée sous ces deux mêmes licences, sans condition supplémentaire.
