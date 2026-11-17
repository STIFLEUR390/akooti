# 0006 — Transport IPC : socket Unix natif, named pipe différé

- **Statut :** acceptée
- **Date :** 2026-11-17

## Contexte

L'ADR 0003 fixe la frontière : le daemon détient la clé maître, `akooti mcp`
n'a qu'un jeton et parle par un socket local. Le document d'architecture
mentionnait `interprocess` comme transport, avec un Unix Socket et un Named
Pipe Windows.

La CI ne construit que sur Linux, et la cible de développement actuelle est
Linux. `interprocess` ajoute une couche d'abstraction (et une dépendance) pour
une seconde plateforme qui n'a pas encore de build ni de test.

## Décision

- Phase 5 : transport **`std::os::unix::net::UnixListener`**, socket dans
  `$XDG_RUNTIME_DIR/akooti/daemon.sock`, répertoire `0o700`, socket `0o600`.
  Aucun port TCP, aucun serveur HTTP.
- Le protocole est **exporté par le crate `akooti-ipc`** (cadrage 4 octets
  little-endian + JSON, plafond 1 Mio) : c'est lui qui change de plateforme,
  pas la politique.
- Sur une plateforme non Unix, `daemon::start` renvoie une raison explicite et
  le mode headless échoue proprement : **aucun succès simulé**.
- Le Named Pipe Windows sera branché dans `akooti-ipc` derrière la même API
  (`socket_path`, `DaemonClient`), sans toucher aux commandes ni aux outils.

## Alternatives écartées

- `interprocess` tout de suite : dépendance et surface non testables dans la
  CI actuelle, pour une plateforme sans build.
- Socket TCP sur `127.0.0.1` avec un secret partagé : surface réseau et
  authentification applicative là où le noyau suffit.
- Garder le stub « not implemented » sans le documenter : l'écart avec
  `docs/architecture.md` deviendrait invisible.

## Conséquences

- `docs/architecture.md` et le README gardent `interprocess` comme cible ; cet
  ADR date l'écart.
- Les tests IPC (`akooti-ipc`) sont `#[cfg(unix)]` : ils couvrent le cadrage, le
  client, les erreurs distantes et le changement de version.
- Quand Windows arrivera : implémenter le transport nommé dans `akooti-ipc`,
  puis activer le job CI correspondant.
