# 0007 — Remplissage d'un compte neuf, secrets TOTP courts, audit fidèle

- **Statut :** acceptée
- **Date :** 2026-11-17
- **Contexte de découverte :** recette manuelle de la phase 5 dans le binaire réel
  (voir [desktop-tests.md](../desktop-tests.md), §5.4)

## Contexte

La phase 5 livre `vault_cdp_autofill`, `vault_create_and_fill` et `vault_exec`
derrière une frontière stricte : l'agent ne reçoit jamais de secret, seulement
une issue d'action. La recette manuelle — agent MCP réel, page de connexion
locale, Chrome headless — a mis en évidence trois écarts entre le comportement
attendu et le comportement observé.

1. **Un secret TOTP de 80 bits était refusé.** La caisse à outils TOTP standard
   exige au moins 128 bits. Beaucoup de services réels (Google Authenticator en
   tête) émettent des secrets de 16 caractères Base32, soit 80 bits.
2. **`vault_create_and_fill` créait le compte sans remplir le formulaire.** Les
   sélecteurs étaient acceptés par le serveur MCP, puis perdus avant d'atteindre
   le daemon ; le remplissage partait donc sans cible.
3. **Le journal d'audit mentait.** Un remplissage qui ne remplissait aucun champ
   était enregistré `success`, deux lignes pouvaient être écrites pour un seul
   appel, et un refus de politique citait le nom d'un helper interne au lieu de
   l'action demandée.

Ces trois points touchent la même promesse : *ce que l'agent lit doit être vrai,
et ce qui est journalisé doit être vérifiable*.

## Décision

### 1. Un compte neuf est rempli avec des sélecteurs explicites

- `vault_create_and_fill` accepte un objet `selectors`
  (`username`, `password`, `totp`, `submit`).
- Ces sélecteurs traversent **toute** la chaîne : serveur MCP → protocole IPC →
  daemon → moteur d'injection. Aucun étage ne les reconstruit ni ne les ignore.
- À défaut de sélecteurs fournis par l'agent, ceux enregistrés sur l'entrée sont
  utilisés.
- L'absence de sélecteur exploitable **ne bloque pas** la création du compte :
  elle produit `status: partial` et un audit `error`, parce que le compte existe
  bel et bien dans le coffre.

### 2. Les secrets TOTP courts sont acceptés

- Le décodage Base32 est le seul refus : un secret qui n'est pas du Base32 valide
  lève `InvalidInput`.
- La construction du générateur passe par `TOTP::new_unchecked` pour ne pas
  imposer le plancher de 128 bits, avec les paramètres fixés localement
  (6 chiffres, pas de 30 s, dérive de ±1 pas).
- Les espaces et tirets sont retirés, la casse est normalisée avant décodage :
  un secret copié depuis un service reste utilisable tel quel.

### 3. L'audit suit la réalité, une action = une ligne

- Une ligne d'audit est écrite **par le daemon**, une seule fois, quand l'issue
  est connue. Le serveur MCP ne journalise pas : il relaie.
- Le statut découle du résultat : `success` si au moins un champ a été rempli,
  `error` si le remplissage a échoué ou n'a touché aucun champ, `denied` en cas
  de refus de politique ou de portée.
- Le détail d'un refus porte le **nom de l'action demandée**
  (`cdp_autofill refused by policy`), jamais celui d'un helper interne.
- Aucune ligne ne contient de valeur secrète : seulement un identifiant
  technique, une origine et un diagnostic court.

## Alternatives écartées

- **Refuser les secrets TOTP courts** au nom de la recommandation RFC 4226 :
  cela rendrait le coffre inutilisable avec les services qui émettent 80 bits,
  pour un gain de sécurité nul (l'entropie est celle du service, pas du client).
- **Compléter les sélecteurs par des heuristiques DOM** dans le daemon : le
  remplissage deviendrait imprévisible, et le diagnostic impossible à expliquer
  à l'agent.
- **Journaliser côté MCP** en plus du daemon : double écriture, doublons et deux
  sources de vérité pour une même action.
- **Journaliser `success` puis corriger** : un audit optimiste est pire qu'une
  absence d'audit, puisqu'il est consulté pour décider.

## Conséquences

- `crates/akooti-core/src/totp.rs` porte un commentaire explicite sur le plancher
  contourné : la règle SEC-10 de [regles.md](../regles.md) le référence.
- `akooti_ipc::Request::VaultCreateAndFill` transporte `selectors` ; toute
  nouvelle surface d'injection doit faire de même (protocole versionné, `1`).
- Les tests de `akooti-core` (`tests/mcp_access_test.rs`) et du daemon couvrent :
  secret de 80 bits accepté, sélecteurs transmis de bout en bout, `partial`
  journalisé en `error`, une seule ligne par action, détail de refus nommant
  l'action réelle.
- Une évolution future (remplissage multi-étapes, sélecteurs dynamiques) devra
  préserver les deux invariants : les sélecteurs de l'agent gagnent sur ceux du
  coffre, et un succès d'audit n'est écrit qu'après un remplissage effectif.
