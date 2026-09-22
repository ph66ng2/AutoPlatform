# AutoPlatform — instruções para agentes

Fonte de verdade dos tickets: `.workflow/workflow.json`.

## Execução

1. Rode `.workflow/scripts/waves.sh plan .workflow/workflow.json`.
2. Implemente somente ticket marcado como liberado.
3. Crie a worktree com `.workflow/scripts/waves.sh spawn .workflow/workflow.json <ID>`.
4. Leia a especificação completa do ticket e não implemente vizinhos.
5. Execute os checks do ticket, revise o diff e deixe pronto para revisão humana.
6. Nunca faça merge, deploy, rollout, backfill ou operação real automaticamente.

Toda worktree parte de `origin/main`; cada ticket usa branch `agent/<id>`. O merge é humano.

## Fronteiras obrigatórias

- AutoPlatform é a fonte única das migrations compartilhadas e contratos canônicos.
- AutoOS e AutoBO acessam APIs versionadas; nenhum produto escreve diretamente nas tabelas internas do outro.
- Queries SQL são runtime e migrations são aditivas e sequenciais; migration aplicada nunca é editada.
- Toda referência entre tenants inclui `company_id` e constraints compostas quando aplicável.
- RLS usa `USING` e `WITH CHECK`; testes sempre incluem tenant A/B.
- Provider tokens usam envelope encryption; a chave-mestra fica no ambiente do serviço.
- Redirect do checkout nunca confirma pagamento.
- Timeout financeiro ou fiscal indeterminado exige consulta antes de retry.
- Fiscal v1 exige permissão `FISCAL` e confirmação humana.
- Estoque usa ledger imutável; saldo nunca é editado diretamente e nunca fica negativo.
- Mutação de estoque offline é proibida.

## Checks esperados após o scaffold Rust

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Tickets com banco também executam testes de integração contra PostgreSQL descartável e verificação de migrations. Credenciais e dados reais nunca entram em fixtures ou logs.

