# Promoção de migrations

Migrations não rodam na subida da API nem do worker. Staging e produção só mudam de schema pelo workflow `promote-migrations`, disparado à mão.

O workflow tem dois jobs excludentes. O alvo `staging` usa o GitHub Environment `staging`. O alvo `production` usa o GitHub Environment `production`. O job que não rodou não recebe o segredo do outro.

Quem dispara escreve `aprovo-migrations-staging` ou `aprovo-migrations-production`, conforme o alvo. Frase diferente, `RUN_MIGRATIONS_ON_STARTUP=true` ou `AUTO_PLATFORM_CI_DATABASE=1` recusam a aplicação. O script não imprime a frase nem a senha do banco.

Os segredos de conexão (`PGHOST`, `PGUSER`, `PGPASSWORD`, `PGDATABASE` e, se precisar, `PGPORT`) ficam no GitHub Environment. Um humano ainda precisa criar esses environments e exigir revisor. Este repositório não faz essa configuração e não dispara a promoção.

O ensaio em pull request aplica a mesma trilha só nos bancos descartáveis `env_promote_staging` e `env_promote_production`. Esse ensaio não promove ambiente real.
