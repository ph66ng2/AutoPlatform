# Backup e restore

Cada backup de staging ou produção leva um rótulo `environment=staging` ou `environment=production` ao lado do arquivo `pg_dump` em formato custom. O restore lê esse rótulo e recusa origem diferente antes de chamar `pg_restore`. O banco de destino permanece como estava.

O ensaio do CI usa só PostgreSQL descartável e bancos `env_drill_*`. O script recusa remover qualquer outro nome. Produção não entra nesse ensaio e não é apagada por ele.

Restore real, quando existir, segue a mesma regra: o rótulo do backup tem de ser o ambiente de destino. Backup de staging não entra em produção, e o inverso também não. Não há promoção automática de dados.
