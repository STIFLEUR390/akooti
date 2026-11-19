import type { RefObject } from "react";

import type { Folder, VaultItem } from "@akooti/shared";

import { folderName, hostOf, initials, itemTypeLabel, matchesQuery } from "../lib/format";
import { Icon } from "./Icons";

interface ItemListProps {
  items: VaultItem[];
  folders: Folder[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  query: string;
  onQuery: (query: string) => void;
  onCreate: () => void;
  onOpenGenerator: () => void;
  searchRef: RefObject<HTMLInputElement | null>;
  loading: boolean;
  activeFolderName: string | null;
}

const TYPE_ICONS = {
  login: "key",
  api_key: "shield",
  note: "note",
  env_set: "terminal",
} as const;

/** Middle column: search plus the filtered entry list. */
export function ItemList({
  items,
  folders,
  selectedId,
  onSelect,
  query,
  onQuery,
  onCreate,
  onOpenGenerator,
  searchRef,
  loading,
  activeFolderName,
}: ItemListProps) {
  const filtered = items.filter((item) => matchesQuery(item, query));

  return (
    <section className="item-list" aria-label="Entrées du coffre">
      <div className="item-list__search">
        <label className="visually-hidden" htmlFor="vault-search">
          Rechercher une entrée
        </label>
        <span className="item-list__search-icon" aria-hidden="true">
          <Icon name="search" size={15} />
        </span>
        <input
          id="vault-search"
          ref={searchRef}
          type="search"
          value={query}
          onChange={(event) => onQuery(event.target.value)}
          placeholder="Rechercher…"
          autoComplete="off"
          spellCheck={false}
        />
        <kbd className="item-list__hint">/</kbd>
      </div>

      <div className="item-list__actions">
        <button type="button" className="button button--primary" onClick={onCreate}>
          <Icon name="plus" size={15} />
          Nouvelle entrée
        </button>
        <button
          type="button"
          className="button"
          onClick={onOpenGenerator}
          title="Générateur (Ctrl G)"
        >
          <Icon name="wand" size={15} />
          Générateur
        </button>
      </div>

      <p className="item-list__context" aria-live="polite">
        {activeFolderName ?? "Toutes les entrées"} · {filtered.length} entrée
        {filtered.length === 1 ? "" : "s"}
        {query ? ` pour « ${query} »` : ""}
      </p>

      {loading ? <p className="empty-state">Chargement du coffre…</p> : null}

      {!loading && filtered.length === 0 ? (
        <div className="empty-state">
          <Icon name="search" size={20} />
          <p>
            {query
              ? "Aucune entrée ne correspond à cette recherche."
              : "Ce dossier est vide. Créez une entrée pour commencer."}
          </p>
        </div>
      ) : null}

      <ul className="item-list__rows">
        {filtered.map((item) => {
          const active = item.id === selectedId;
          return (
            <li key={item.id}>
              <button
                type="button"
                className={active ? "item-row item-row--active" : "item-row"}
                onClick={() => onSelect(item.id)}
                aria-current={active ? "true" : undefined}
                title={item.title}
              >
                <span className="item-row__avatar" aria-hidden="true">
                  {initials(item.title)}
                </span>
                <span className="item-row__body">
                  <span className="item-row__title">
                    {item.title}
                    {item.favorite ? (
                      <span className="item-row__star" title="Favori">
                        <Icon name="star" size={12} />
                      </span>
                    ) : null}
                  </span>
                  <span className="item-row__meta">
                    <Icon name={TYPE_ICONS[item.itemType]} size={11} />
                    {hostOf(item.primaryUrl) || itemTypeLabel(item.itemType)}
                  </span>
                  <span className="item-row__folders">
                    {item.folderIds.map((folderId) => (
                      <span key={folderId} className="chip chip--quiet">
                        {folderName(folders, folderId)}
                      </span>
                    ))}
                  </span>
                </span>
              </button>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
