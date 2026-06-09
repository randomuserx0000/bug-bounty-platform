# OSINT Academy — diseño (fase futura)

> Estado: **diseño documentado, no implementado**. El producto OSINT (envío,
> revisión, catálogo, compra) ya está en la plataforma; el módulo de formación
> que sigue es la siguiente capa para captar y capacitar nuevos investigadores.

## Objetivo

Captar talento local sin experiencia y convertirlo en investigadores OSINT que
vendan sus hallazgos en Escudo Digital. El que no sabe nada compra un curso;
al aprobarlo, queda habilitado para vender informes OSINT en la plataforma.

## Flujo

```
visitante → compra/se inscribe en curso → completa módulos → aprueba evaluación
          → badge "OSINT Certified" → habilitado para vender informes OSINT
```

## Modelo de datos propuesto

```sql
CREATE TYPE course_level AS ENUM ('intro','intermediate','advanced');

CREATE TABLE courses (
    id           UUID PRIMARY KEY,
    slug         CITEXT NOT NULL UNIQUE,
    title        TEXT NOT NULL,
    summary      TEXT NOT NULL,
    level        course_level NOT NULL DEFAULT 'intro',
    price_cents  INTEGER NOT NULL DEFAULT 0,   -- 0 = gratis / becado por aliados
    published    BOOLEAN NOT NULL DEFAULT FALSE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE course_modules (
    id          UUID PRIMARY KEY,
    course_id   UUID NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    title       TEXT NOT NULL,
    body_md     TEXT NOT NULL,                 -- contenido (markdown/video embed)
    UNIQUE (course_id, position)
);

CREATE TABLE course_enrollments (
    course_id    UUID NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    user_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    progress_pct INTEGER NOT NULL DEFAULT 0,
    passed_at    TIMESTAMPTZ,                  -- aprobó la evaluación final
    certified    BOOLEAN NOT NULL DEFAULT FALSE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (course_id, user_id)
);
```

## Integración con el producto OSINT

- Gate opcional en `/osint/new`: exigir `certified = TRUE` en algún
  `course_enrollment` antes de permitir vender (configurable por la plataforma).
- Badge "OSINT Certified" en el perfil del investigador (reusa la sección de
  achievements existente en `/settings/profile`).
- Becas: aliados de la red REDSEG patrocinan cupos (`price_cents = 0`).

## Pendiente para implementar

- Migración `0005_courses.sql`, dominio `src/domain/course.rs`,
  `src/db/courses.rs`, rutas `src/routes/courses.rs`, templates `templates/courses/`.
- Pasarela de pago para cursos de pago (reusar rails existentes o checkout aparte).
- Evaluación/quiz y emisión de certificado.
