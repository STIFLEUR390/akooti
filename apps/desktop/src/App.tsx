import { SYSTEM_FOLDERS } from "@akooti/shared";

/**
 * Placeholder shell. The vault UI (unlock screen, sidebar, item detail,
 * generator, import wizard) lands in phase 4.
 */
export function App() {
  return (
    <main className="shell">
      <header>
        <h1>Akooti</h1>
        <p className="tagline">Coffre-fort local-first pour les agents IA</p>
      </header>

      <section aria-labelledby="folders-heading">
        <h2 id="folders-heading">Dossiers système</h2>
        <ul className="folders">
          {SYSTEM_FOLDERS.map((folder) => (
            <li key={folder.id} style={{ borderColor: folder.color }}>
              <span className="folder-name">{folder.name}</span>
              <code>{folder.slug}</code>
            </li>
          ))}
        </ul>
      </section>

      <footer>
        <p>Interface en construction — phase 0 (fondations).</p>
      </footer>
    </main>
  );
}
