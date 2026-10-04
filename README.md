# Pixelify

Application de bureau Rust pour convertir des images en pixel art, supprimer des arrière-plans et exporter des assets de jeu.

## Lancer

```sh
cargo run --release
```

## Vérifier le moteur

```sh
cargo test
```

Pixelify enregistre ses projets en `.pixelify`, un fichier portable qui conserve les calques, frames capturées, durées et réglages. Les exports image incluent PNG, JPEG, WebP, BMP, TGA et TIFF. L’export `.aseprite` contient les calques RGBA et plusieurs frames. JPEG aplatit la transparence sur blanc.

## Application macOS

```sh
sh scripts/build-macos.sh
open dist/Pixelify.app
```

Le script produit `dist/Pixelify.app` et `dist/Pixelify-macOS.zip`. L’archive est créée à partir d’un paquet vérifié hors des dossiers synchronisés, qui peuvent réinjecter des métadonnées Finder. Le paquet est signé de façon ad hoc, et non notarisé pour une distribution publique. Le binaire de développement reste disponible dans `target/release/pixelify`.

## Atelier

- Importer un fichier ou charger « Image d’exemple » pour découvrir les outils.
- Choisir un style, régler l’échelle, puis « Appliquer le style ». Game Boy et PICO-8 utilisent de vraies palettes de couleurs.
- Comparer « Rendu » et « Original » ; zoomer à la molette et déplacer le canevas par glissement.
- Retirer un fond uniforme, puis effacer/restaurer au pinceau.
- Capturer des frames et cliquer leurs cartes pour les consulter. Les captures sont conservées à l’enregistrement.
- Cmd/Ctrl Z : annuler ; Cmd/Ctrl Shift Z : rétablir ; Cmd/Ctrl S : enregistrer.

La suppression de fond actuelle est basée sur la couleur des bords, et non sur un modèle de segmentation IA. La timeline est une collection de captures ; elle ne propose pas encore de lecture animée, de tags ou d’édition complète de chaque frame. Les spritesheets et le traitement par lots restent à implémenter : cette version ne constitue pas encore la totalité du périmètre 1.2 prévu.

## Personnages LPC en local

Pixelify peut composer un personnage LPC habillé sur sa spritesheet animée standard (832 × 3456 px). Les œuvres ne sont pas incluses dans Pixelify : l’application ne télécharge ni ne redistribue les assets ni le code du générateur.

1. Clonez localement le dépôt [Universal LPC Spritesheet Character Generator](https://github.com/LiberatedPixelCup/Universal-LPC-Spritesheet-Character-Generator), avec le dossier `spritesheets` complet.
2. Placez-le dans `assets/lpc` pour un chargement automatique, ou dans Pixelify ouvrez **Personnage LPC**, puis **Choisir la bibliothèque LPC…** et sélectionnez le dossier du dépôt. Il doit contenir `spritesheets` et `sheet_definitions`.
3. Choisissez la morphologie, le dossier d’assets, l’emplacement, une pièce et sa variante. L’aperçu lit l’animation sélectionnée à 8 images/s et affiche les directions nord, ouest, sud et est côte à côte.
4. Exportez avec **Exporter le personnage + crédits**. L’archive contient `character.png` et `CREDITS.txt`. Enregistrez également le `.pixelify` : il conserve ces crédits.

LPC mélange plusieurs licences par asset (CC0, CC-BY, CC-BY-SA, OGA-BY et GPL selon les fichiers). Un usage privé ne supprime pas les obligations de crédit ni, le cas échéant, de partage à l’identique. Consultez toujours le `CREDITS.csv` du dépôt en plus du fichier de crédits généré. L’intégration couvre les 15 animations standard de la feuille LPC. Les animations spéciales surdimensionnées d’armes/outils restent séparées de cette feuille standard.

## Direction visuelle

Système visuel natif dans `src/design.rs` : police Inter (licence OFL incluse), icônes vectorielles [Lucide](https://lucide.dev) (MIT et ISC), surfaces claires, cartes arrondies, accent indigo, navigation textuelle et actions distinctes des réglages.

Références consultées :

- [Workforce Management — Airzon Agency](https://dribbble.com/shots/27304614-Workforce-Management-Clean-SaaS-Calendar-Dashboard-UI)
- [Modern, Clean SaaS Dashboard — Nipa Akter](https://dribbble.com/shots/26642812-Modern-Clean-SaaS-Dashboard-Design)
