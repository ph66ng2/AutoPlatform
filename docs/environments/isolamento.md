# Isolamento de staging e produção

Staging e produção são ambientes separados. Este repositório descreve a matriz e recusa configuração compartilhada. Não cria projeto Supabase, banco, backup, coletor de log nem canal de alerta.

## Matriz

Os arquivos em `infra/environments/` são o contrato versionado:

- `local.env` espelha `.env.example`.
- `staging.env` e `production.env` usam `APP_ENV` igual ao nome do arquivo.
- `RUN_MIGRATIONS_ON_STARTUP=false` nos três. O processo recusa iniciar se o valor for verdadeiro.
- `DATABASE_URL`, `SUPABASE_URL`, `SUPABASE_ANON_KEY` e `SUPABASE_SERVICE_ROLE_KEY` ficam vazios no Git.
- `SUPABASE_PROJECT_REF`, `BACKUP_TARGET`, `LOG_SINK` e `ALERT_CHANNEL` são identificadores distintos por ambiente. O sufixo `unprovisioned` marca que o recurso ainda não existe.

Valores vazios não contam como chave compartilhada. Qualquer segredo preenchido, identificador repetido ou valor com o nome do projeto interno é recusado por `scripts/ci/environments.sh`.

## O que um humano faz depois

Quando houver autorização registrada no ticket:

1. Criar projetos vazios e separados para staging e produção.
2. Gravar as chaves só nos GitHub Environments `staging` e `production`.
3. Configurar revisores obrigatórios nesses environments. O repositório não consegue fazer isso.
4. Atualizar os identificadores da matriz no mesmo PR que registra a confirmação. Sem essa confirmação, o provisionamento não começa.

Não reutilizar o projeto interno. Não copiar chave de um ambiente para o outro. Não apagar produção por automação.
