-- =====================================================================
--  Akooti — schéma Supabase pour la synchronisation chiffrée de bout en bout
-- =====================================================================
--
--  À FAIRE UNE FOIS, dans VOTRE projet Supabase :
--
--   1. Ouvrir  https://supabase.com/dashboard/project/_/sql/new
--      (ou : Dashboard → SQL Editor → New query)
--   2. Copier TOUT ce fichier, le coller dans l'éditeur.
--   3. Cliquer « Run ».
--
--  Le script est idempotent : il peut être rejoué sans rien casser (utile
--  après une mise à jour d'Akooti). Il ne crée aucune donnée utilisateur.
--
--  Ce que le serveur verra ensuite : uniquement des blobs chiffrés
--  (`ciphertext`, `nonce`) et des métadonnées techniques. Jamais un titre,
--  une URL, un nom de dossier ni un secret — voir docs/schema.md §3.
--
--  Ne jamais utiliser la clé `service_role` / `sb_secret_…` dans Akooti.
--  Seules l'URL du projet et la clé `anon` / `sb_publishable_…` y entrent.
-- =====================================================================

create extension if not exists "pgcrypto";

-- ---------------------------------------------------------------------
-- Enveloppe cryptographique du coffre (une par utilisateur)
-- Paramètres Argon2id publics + clé maître encapsulée : rien de lisible.
-- ---------------------------------------------------------------------
create table if not exists public.akooti_vault_envelopes (
    user_id uuid primary key references auth.users(id) on delete cascade,
    vault_id uuid not null,
    kdf_algorithm text not null default 'argon2id',
    kdf_salt text not null,
    kdf_memory_kib integer not null default 65536,
    kdf_iterations integer not null default 3,
    kdf_parallelism integer not null default 4,
    wrapped_master_key text not null,
    wrapped_key_nonce text not null,
    updated_at timestamptz not null default now()
);

-- ---------------------------------------------------------------------
-- Enregistrements chiffrés : dossiers, entrées, passkeys et règles ont
-- tous la même forme, donc le serveur ne peut pas compter les dossiers.
-- ---------------------------------------------------------------------
create table if not exists public.akooti_sync_records (
    id uuid not null,
    user_id uuid not null references auth.users(id) on delete cascade,
    record_type text not null check (record_type in ('folder', 'item', 'passkey', 'routing_rule')),
    ciphertext text not null,
    nonce text not null,
    version bigint not null default 1,
    client_updated_at timestamptz not null,
    server_updated_at timestamptz not null default now(),
    is_deleted boolean not null default false,
    primary key (user_id, id)
);

create index if not exists idx_sync_records_delta
    on public.akooti_sync_records (user_id, server_updated_at);

-- ---------------------------------------------------------------------
-- `server_updated_at` est le curseur de synchronisation : il avance à
-- chaque écriture (jamais à la lecture) ; `version` est bumpée ici pour
-- que les appareils n'aient pas à se coordonner.
-- ---------------------------------------------------------------------
create or replace function public.akooti_touch_sync_record()
returns trigger
language plpgsql
as $$
begin
    new.server_updated_at := now();
    if tg_op = 'UPDATE' then
        new.version := old.version + 1;
    end if;
    return new;
end;
$$;

drop trigger if exists trg_akooti_sync_timestamp on public.akooti_sync_records;
create trigger trg_akooti_sync_timestamp
    before insert or update on public.akooti_sync_records
    for each row execute function public.akooti_touch_sync_record();

-- ---------------------------------------------------------------------
-- RLS : un utilisateur ne lit et n'écrit que ses propres lignes, même
-- avec la clé anon.
-- ---------------------------------------------------------------------
alter table public.akooti_vault_envelopes enable row level security;
alter table public.akooti_sync_records enable row level security;

drop policy if exists "Users manage own vault envelope" on public.akooti_vault_envelopes;
create policy "Users manage own vault envelope"
    on public.akooti_vault_envelopes for all
    using (auth.uid() = user_id)
    with check (auth.uid() = user_id);

drop policy if exists "Users manage own sync records" on public.akooti_sync_records;
create policy "Users manage own sync records"
    on public.akooti_sync_records for all
    using (auth.uid() = user_id)
    with check (auth.uid() = user_id);

-- PostgREST s'exécute sous le rôle `authenticated` ; on lui donne les
-- droits de table, la RLS restant le vrai verrou. `anon` n'obtient rien.
grant select, insert, update, delete on public.akooti_vault_envelopes to authenticated;
grant select, insert, update, delete on public.akooti_sync_records to authenticated;

-- ---------------------------------------------------------------------
-- Realtime : la même RLS s'applique aux événements de changement.
-- ---------------------------------------------------------------------
do $$
begin
    if not exists (
        select 1 from pg_publication_tables
        where pubname = 'supabase_realtime'
          and schemaname = 'public'
          and tablename = 'akooti_sync_records'
    ) then
        alter publication supabase_realtime add table public.akooti_sync_records;
    end if;
end;
$$;

-- Recharge le cache de schéma de PostgREST pour que les tables soient
-- visibles immédiatement (sinon attendre quelques secondes).
notify pgrst, 'reload schema';

-- ---------------------------------------------------------------------
-- Vérification : ces requêtes doivent renvoyer les objets ci-dessus.
-- ---------------------------------------------------------------------
select 'akooti_vault_envelopes' as objet, count(*) as lignes
from public.akooti_vault_envelopes
union all
select 'akooti_sync_records', count(*) from public.akooti_sync_records;

select schemaname, tablename, policyname, cmd
from pg_policies
where schemaname = 'public' and tablename like 'akooti_%'
order by tablename, policyname;
