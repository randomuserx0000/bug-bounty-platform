# Academia — curso "Analista de Ciberseguridad" (diseño + v1 de captación)

> Estado: **landing + captación de leads implementadas; LMS pendiente**.
> El producto OSINT (envío, revisión, catálogo, compra) ya está en la
> plataforma. La oferta formativa se amplió de "curso OSINT" a un curso de
> iniciación **Analista de Ciberseguridad** con 3 pilares: fundamentos de
> ethical hacking, pentesting y OSINT.

## Objetivo

Captar talento local sin experiencia y convertirlo en investigadores
certificados que vendan sus hallazgos en Escudo Digital. El que no sabe nada
compra el curso de iniciación (precio accesible, `COURSE_ANALISTA_CENTS` en
`domain/pricing.rs`); al aprobarlo, entra al staff de investigadores
certificados y queda habilitado para vender informes en la plataforma.

## Implementado (v1 — captación)

- Landing pública `/cursos/analista-ciberseguridad` (3 pilares, precio de
  lanzamiento, formulario "Solicitar curso" con honeypot y rate-limit por IP).
- `POST /cursos/solicitar` guarda el lead en `course_requests`
  (migración `0005_course_requests.sql`) y avisa al buzón de la plataforma
  (`osint_notify_email`). Inscripción y cobro se gestionan operativamente.
- CTA "Solicitar curso" en la tarjeta "🎓 Fórmate desde cero" de la home.

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

- Vista admin de `course_requests` (hoy los leads llegan por email y quedan en BD).
- Migración de cursos/LMS (`courses`, `course_modules`, `course_enrollments`),
  dominio `src/domain/course.rs` y contenido de los módulos.
- Pasarela de pago para cursos de pago (reusar rails existentes o checkout aparte).
- Evaluación/quiz y emisión de certificado + badge en el perfil.
