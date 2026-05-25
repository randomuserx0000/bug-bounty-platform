-- Sign-In con Google (y futuros providers: GitHub, GitLab, ...).
--
-- Decisión: en vez de tabla separada `oauth_identities`, agregamos dos
-- columnas al row de users. Eso fuerza 1 cuenta = 1 identidad OAuth, que
-- alcanza para el MVP. Si en el futuro queremos que el mismo user pueda
-- enlazar varios providers (Google + GitHub), migrar a tabla aparte.
--
-- `oauth_subject` es el `sub` claim del id_token (string opaco, estable
-- por user dentro del provider). No usamos el email como llave porque
-- los emails de Google pueden cambiar de alias (workspace), aunque el
-- `sub` no cambia nunca.

ALTER TABLE users
    ADD COLUMN oauth_provider TEXT,
    ADD COLUMN oauth_subject  TEXT;

CREATE UNIQUE INDEX uq_users_oauth
    ON users(oauth_provider, oauth_subject)
    WHERE oauth_provider IS NOT NULL;
