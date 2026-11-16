# Couverture des fonctions critiques

Les tests du cœur s'exécutent sans Tauri :

```bash
cargo test -p akooti-core --locked
cargo clippy -p akooti-core --all-targets --locked -- -D warnings
```

| Surface | Contrats couverts | Emplacement |
| --- | --- | --- |
| Crypto | KDF, nonce, AAD, mauvais mot de passe, ciphertext altéré | `src/crypto.rs`, `tests/vault_flow.rs` |
| SQLCipher | PRAGMAs, migration idempotente, seeds, cascade, absence de clair sur disque | `src/db.rs`, `tests/vault_flow.rs` |
| Cycle du coffre | création, verrouillage, réouverture, inactivité | `src/session.rs`, `tests/vault_flow.rs` |
| DAO | N:N, soft-delete, remplacement des métadonnées, historique chiffré | `tests/vault_flow.rs`, `tests/phase2_vault_test.rs` |
| Générateur | classes de caractères, ambiguïté, EFF Diceware, entropie | `tests/generator_test.rs` |
| TOTP | vecteurs RFC 6238 SHA-1/SHA-256, skew, secret chiffré | `tests/phase2_vault_test.rs` |
| Routage | suffixe, host exact, CIDR, regex, faux positifs, rétroactif non destructif | `tests/routing_test.rs` |

Les fixtures utilisent des coffres temporaires isolés créés avec `tempfile`.
Aucune fixture commitée ne contient de secret réel.
