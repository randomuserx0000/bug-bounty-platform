-- Escudo Digital :: solicitudes del curso "Analista de Ciberseguridad"
--
-- Captación de leads para la academia (ver docs/osint-academy.md). El LMS
-- completo (courses, modules, enrollments) sigue siendo fase futura; aquí
-- solo registramos a quién le interesa el curso para contactarlo, mientras
-- la inscripción y el cobro se gestionan operativamente.

CREATE TABLE course_requests (
    id          UUID PRIMARY KEY,
    -- Si el visitante ya tiene cuenta la enlazamos; si no, queda el contacto.
    user_id     UUID REFERENCES users(id) ON DELETE SET NULL,
    name        TEXT NOT NULL,
    email       CITEXT NOT NULL,
    -- Nivel autodeclarado: 'none' | 'basic' | 'intermediate'.
    experience  TEXT NOT NULL DEFAULT 'none',
    message     TEXT NOT NULL DEFAULT '',
    course_slug TEXT NOT NULL DEFAULT 'analista-ciberseguridad',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_course_requests_created ON course_requests(created_at DESC);
