-- Baseline do runner de migrations. Sem tabelas de negócio.
CREATE SCHEMA IF NOT EXISTS platform;

COMMENT ON SCHEMA platform IS 'Infraestrutura da plataforma. Tabelas de negócio entram em tickets posteriores.';
