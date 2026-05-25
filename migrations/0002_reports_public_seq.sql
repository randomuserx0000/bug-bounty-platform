-- Sequence para generar public_id de reports tipo "VE-2026-00042".
--
-- Decisión: contador GLOBAL, no por año. El número crece monotónico y se
-- prefijea con el año actual al insertar. Esto da IDs únicos sin tabla
-- counter ni race condition, a costa de que los números no resetean en
-- enero. Suficiente para MVP; si más adelante queremos #1 cada enero
-- migramos a tabla `report_counters(year, last_n)` con UPSERT.

CREATE SEQUENCE IF NOT EXISTS reports_public_seq START 1;
