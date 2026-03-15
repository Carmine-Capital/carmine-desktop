# UI/UX Refonte — Carmine Desktop

Date: 2026-03-15
Status: Draft

## Objective

Refonte complète de l'interface Carmine Desktop (wizard + settings) pour un look compact, minimaliste et haut de gamme. Style inspiré de Linear/Arc et de l'app Handy (Parakeet V3). Deux phases : visual/structure d'abord, JS refactor ensuite.

## Phasage

### Phase 1 — Visual & Structure

Refonte du HTML et CSS des deux vues (wizard.html, settings.html). Le JS existant est adapté pour les nouveaux sélecteurs/IDs mais la logique métier reste identique. Résultat : le nouveau look est en place, tout fonctionne.

### Phase 2 — JS Cleanup

Refactoring du JS : état centralisé (un objet state par vue), suppression des variables globales, fonctions `render()` déclaratives qui reconstruisent le DOM à partir du state. Pas de framework, on reste en vanilla JS.

## Design System

### Couleurs (tokens CSS)

Backgrounds relevés d'un cran par rapport à l'actuel :

| Token | Ancien | Nouveau | Usage |
|-------|--------|---------|-------|
| `--bg-base` | `#0e0f14` | `#121318` | Body, fond principal |
| `--bg-surface` | `#16181f` | `#151620` | Sidebar |
| `--bg-elevated` | `#1e2028` | `#1a1b24` | Inputs, selects, éléments surélevés |
| `--border` | `#2a2d3a` | `rgba(255,255,255,0.04)` | Séparateurs quasi-invisibles |
| `--accent` | `#99222E` | `#99222E` | Inchangé |
| `--accent-hover` | `#b52a38` | `#b52a38` | Inchangé |
| `--text-primary` | `#edeef2` | `#d4d5de` | Texte principal (légèrement atténué) |
| `--text-secondary` | `#8b8fa8` | `#6b6f85` | Labels, texte secondaire |
| `--text-muted` | `#5c607a` | `#3d3f54` | Sous-titres, hints |

### Typographie

- Body/labels : 13px (au lieu de 15px)
- Sous-titres descriptifs : 11px, couleur `--text-muted`
- Section headings : 11px, uppercase, `letter-spacing: 0.08em`, couleur `--text-muted`
- Titres de pages : 14-16px, font-weight 600

### Composants

**Toggles** : 30x16px (au lieu de 40x22px). Thumb 12px. Fond accent quand actif, fond `--bg-elevated` quand inactif.

**Boutons** :
- Primaire : fond `rgba(153,34,46,0.85)`, texte blanc, padding `7px 18px`, font-size 12.5px, border-radius 6px
- Ghost/secondaire : texte `--text-secondary`, bordure `rgba(255,255,255,0.05)`, fond transparent
- Destructif : texte `--accent` (rouge), pas de fond
- Texte : pas de bordure ni fond, underline ou couleur seule

**Selects/Inputs** : fond `--bg-elevated`, bordure `rgba(255,255,255,0.05)`, padding compact `4px 10px`, font-size 11.5px, border-radius 5px.

**Settings rows** : pas de cartes conteneurs. Chaque setting est une ligne flex avec label+sous-titre à gauche, contrôle à droite. Séparateur `border-bottom: 1px solid rgba(255,255,255,0.03)`. Padding vertical `9px`.

**Section headings** : texte uppercase 11px, couleur muted, espacement `margin: 22px 0 14px`. Pas de border-bottom (contrairement à l'actuel).

**Sidebar nav-item actif** : pill arrondie `border-radius: 6px` avec fond `rgba(153,34,46,0.85)`, texte blanc. Inactif : texte muted, pas de fond.

### Éléments supprimés

- `.settings-group` (cartes avec bordures) — remplacé par rows plats
- `.tabs` / `.tab` (CSS legacy) — déjà inutilisé
- Bouton Save et badge "Unsaved changes" — remplacés par auto-save
- Hero gradient sur le wizard welcome — supprimé
- `.btn-remove` avec bordure rouge — remplacé par icône trash muted

## Settings — Structure

### Sidebar (190px, au lieu de 220px)

- **Header** : icône "C" carrée arrondie (gradient `135deg, #99222E → #c4354a`, border-radius 6px) + texte "Carmine", font-weight 600, 13px
- **Nav items** : icônes 15px + label 12.5px, gap 8px, padding `7px 12px`
- **Footer** : email 11px muted + "Sign Out" en texte accent, séparé par border-top subtile

### Panel General

Rows plats organisés en sections :

**GENERAL**
- Start on login — toggle + sous-titre "Launch Carmine when you sign in"
- Notifications — toggle + sous-titre "Show sync and error alerts"
- Show in Explorer nav pane — toggle (Windows only, caché sinon)
- Sync interval — select compact + sous-titre "How often to check for remote changes"

**FILE ASSOCIATIONS**
- Liste de rows : `.ext` (bold) + nom de l'app (muted) à gauche, bouton "Override" ghost à droite
- Interaction Override : clic sur "Override" → le bouton est remplacé par un inline input (10rem) + bouton "Set" (ghost) + bouton "Clear" (texte muted). L'input accepte un identifiant d'application (ex: `libreoffice-writer.desktop`). "Set" persiste via `save_file_handler_override`, "Clear" restaure le handler par défaut via `clear_file_handler_override`. L'inline input se ferme après l'action.
- "Re-detect handlers" en texte link en bas

**ADVANCED** (pas de collapsible — tout visible, cohérent avec le design compact)
- Cache directory — label + sous-titre "Where downloaded files are stored" + input text compact à droite
- Cache size limit — label + sous-titre descriptif + input text compact (placeholder "5GB") + bouton "Clear" en texte accent
- Metadata TTL — label + sous-titre "Seconds before re-fetching folder listings" + input compact
- Log level — label + sous-titre "Verbosity of diagnostic output" + select compact

### Panel Mounts

- Section heading "MOUNTS"
- Rows plats : nom du mount + path en sous-titre à gauche, toggle + icône trash à droite
- Mount désactivé : texte atténué, toggle off
- Bouton "Add mount" ghost en bas (icône + 12px, bordure fine)

### Panel About

- Row simple : "Carmine Desktop" + version
- Attribution WinFsp en texte muted

### Auto-save

- Chaque modification de toggle/select/input déclenche un `save_settings` immédiat (debounce ~500ms pour les inputs texte)
- Pas de feedback en cas de succès
- Status bar erreur seule en cas d'échec
- Les toggles de mount (`toggle_mount`) restent en appel immédiat comme actuellement

## Wizard — Structure

### Layout

Même layout sidebar + contenu que les settings. La sidebar sert de stepper vertical (4 étapes). La fenêtre wizard utilise les mêmes dimensions que la fenêtre settings (défini dans la config Tauri).

### Sidebar stepper

- Même logo "C" + "Carmine" en haut
- Label "SETUP" en section heading
- 4 steps listées : numéro dans un cercle + label
  - Step courante : pill accent, numéro sur fond `rgba(255,255,255,0.15)`
  - Steps passées : check vert (`#22c55e`) dans le cercle, texte `--text-secondary`
  - Steps futures : cercle bordure muted, texte très muted
- Pas de clic libre sur les steps (flow linéaire)
- Footer contextuel par step :
  - Step 1 : vide (pas de footer)
  - Step 2 : vide
  - Step 3 : compteur "N sources added"
  - Step 4 : vide

### Step 1 — Welcome

- Titre "Welcome to Carmine" (18px, 600)
- Sous-titre explicatif (13px, muted, max-width 320px)
- Bouton "Sign In with Microsoft" (primaire)
- Pas de hero/gradient

### Step 2 — Signing In

- Titre "Signing in..." (16px)
- Sous-titre "Complete the sign-in in your browser"
- Spinner compact (20px) + texte "Waiting for authentication..." + countdown
- URL fallback en row : input monospace tronqué + bouton "Copy" ghost
- Hint muted "Browser didn't open? Copy the link and paste it manually."
- "Cancel" en texte link muted

### Step 3 — Add Sources

Deux sous-vues (comme actuellement) :

**Vue sites** :
- OneDrive toggle en row plate (comme un setting)
- Barre de recherche SharePoint + liste de sites en rows plats
- Sites suivis listés en rows
- "Sign in with a different account" en texte link muted en bas de la vue sites

**Vue libraries** (après sélection d'un site) :
- Back link "← Back to sites" en texte accent
- Titre du site sélectionné (14px, 600)
- Sous-titre "Select libraries to mount as local folders"
- Libraries en rows avec checkbox custom (carré arrondi 16px, checked = accent, unchecked = bordure muted)
- Libraries déjà montées : opacité réduite, check vert, label "Already mounted"
- Bouton "Add N selected" (primaire) en bas

**Mount creation** : les mounts sont créés au step 3 — `add_mount` est appelé quand l'utilisateur active le toggle OneDrive ou clique "Add N selected" pour les libraries SharePoint. La transition vers le step 4 se fait après que les mounts ont été créés avec succès.

### Step 4 — Done

- Titre "You're all set" (16px)
- Sous-titre explicatif
- Résumé des mounts ajoutés en rows plats (nom + path)
- Bouton "Get Started" (primaire) — appelle `complete_wizard` et ferme la fenêtre
- Hint muted "Carmine will continue running in the system tray."

## Status Bar

Conservée en bas de page, même comportement. Utilisée uniquement pour les erreurs (auto-save silencieux en succès). Styles ajustés pour matcher le nouveau design (font-size 12px, padding réduit).

## CSS Cleanup

Supprimer :
- `.tabs`, `.tab`, `.tab.active` (legacy, inutilisé)
- `.settings-group` (remplacé par rows plats)
- `.welcome-hero`, `.welcome-hero::before` (hero gradient supprimé)
- `.btn-remove` (wizard — migré vers le `.btn-icon` existant)
- `.unsaved-badge` (auto-save, plus besoin)
- `.wizard-container` centré (remplacé par sidebar layout)
- `.source-card` avec checkbox (remplacé par rows plats)

Ajouter :
- `.setting-row` — flex row pour chaque paramètre
- `.setting-label` — label + sous-titre
- `.setting-sub` — sous-titre descriptif
- `.section-heading` — refactorisé (plus de border-bottom)
- `.stepper-item`, `.stepper-item.active`, `.stepper-item.done` — items du wizard stepper
- `.step-number` — cercle numéroté dans le stepper

## JS Changes (Phase 1 — adaptations minimales)

- Mettre à jour les sélecteurs (`querySelector`, `getElementById`) pour correspondre aux nouveaux IDs/classes
- Implémenter l'auto-save : ajouter des event listeners `change` sur chaque input/toggle/select qui appellent `saveSettings()` avec debounce
- Supprimer la logique dirty tracking (`_savedValues`, `checkDirty()`, badge unsaved)
- Supprimer le listener et le DOM du bouton Save
- Conserver le listener `listen('refresh-settings')` qui recharge settings et mounts depuis le backend
- Conserver le listener `listen('navigate-add-mount')` dans le wizard

## JS Changes (Phase 2 — refactoring)

### settings.js

- État centralisé : `const state = { settings: {}, mounts: [], handlers: [] }`
- Fonctions `render*()` qui reconstruisent le DOM à partir du state
- Un seul point d'entrée `init()` qui charge le state et appelle les renderers
- Event listeners attachés par délégation sur le conteneur principal
- Auto-save via `setState()` wrapper qui met à jour le state, re-render, et persiste

### wizard.js

- État centralisé : `const state = { step: 1, authenticated: false, selectedSources: new Map(), ... }`
- Fonctions `renderStep()`, `renderSidebar()` qui réagissent au state
- Suppression des 10+ variables globales
- Navigation entre steps via `goToStep(n)` qui met à jour le state
- Cleanup automatique des listeners via délégation (supprime le pattern `activeListeners`)

## Constraints

- CSP `script-src 'self'` : pas d'inline handlers (addEventListener uniquement)
- Pas de framework JS
- Support des 3 OS (Linux, macOS, Windows) — le champ "Show in Explorer nav pane" reste conditionnel
- ARIA roles et keyboard navigation maintenus (tab/tabpanel pour settings nav, role=checkbox pour library selection)
