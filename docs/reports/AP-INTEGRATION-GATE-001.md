# AP-INTEGRATION-GATE-001 — evidência contratual do backend

## Escopo executado

O teste `tests/e2e/integration/product_sync_gate.rs` usa PostgreSQL descartável,
as migrations 0001–0007 e clientes sintéticos AutoOS/AutoBO. Exercita o núcleo
real de ingestão `FatoComercial v2`, a leitura fiscal com RLS e os comandos reais
de estoque. O fake AutoBO valida um `EstadoFinanceiro v1` com estado `pending`;
esse estado é uma intenção de revisão dentro do teste, não uma confirmação de
pagamento nem um registro financeiro persistido.

| Cenário | Evidência esperada no teste |
| --- | --- |
| Publicação AutoOS A/B | Um snapshot por tenant; `fact_id`, `event_id` e correlação preservados. |
| Resposta perdida e retry | Busca por correlação retorna o fato; retry devolve `created=false`; total de snapshots permanece 2. |
| AutoBO fiscal | Leitura do tenant A encontra o fato; tenant B recebe `NotFound`; uma intenção sintética `pending` valida o contrato v1. |
| Estoque | Reserva repetida devolve o mesmo ID e produz um comando; A conserva 1 unidade disponível, B conserva 5. Evento de estoque valida o contrato v1 com a mesma correlação. |
| Indisponibilidade | Banco de fixtures desabilitado e sessão encerrada; novas conexões falham. Após reabertura e reconexão, fato e saldo continuam consultáveis e sem duplicidade. |
| Rota HTTP publicada | A imagem aceita publicação e replay de A/B (201/200), nega leitura cruzada (404), mantém dois snapshots e responde 503 na ingestão e no readiness com banco parado, enquanto `/health` continua 200. |

## Identificadores e schemas reproduzíveis

- Tenant A: `00000000-0000-4000-8000-0000000000a1`; tenant B: `00000000-0000-4000-8000-0000000000d1`.
- Fato A: `00000000-0000-0000-0000-0000000001a1`; evento A: `00000000-0000-0000-0000-0000000002a1`; OS A: `00000000-0000-0000-0000-0000000003a1`.
- Correlação A: `corr-gate-synthetic-00000000000000a1`; reserva A: `idem-gate-reserve-a-0001`.
- `fato-comercial.schema.json` v2 SHA-256: `1ec7dc2e5355244c9d5ae2e5ff0372ca70c17b8196759ea434a80c8df2b462c5`.
- `estado-financeiro.schema.json` v1 SHA-256: `ebfd2526dc3696eb0e37ae29e316048df756713776e379f963adbe244aaef2e6`.
- `estoque.schema.json` v1 SHA-256: `3cacadcf3102c7bb6304f47e8d16761e78fc4d1e1b42e1f66766aad51fb72397`.

O teste compara o hash v2 com `contracts/v2/manifest.sha256`, confere os hashes
v1 usados e verifica os campos obrigatórios, constantes, enums e UUIDs dos
envelopes. A validação completa dos contratos v1 permanece no gate
`scripts/ci/contracts.sh`.

## Execução e decisão

```bash
AUTO_PLATFORM_CI_DATABASE=1 PGHOST=127.0.0.1 PGPORT=5432 \
  PGUSER=ci PGPASSWORD=ci cargo test -p ap-api --test product_sync_gate
```

Executar somente contra PostgreSQL descartável. O teste cria e remove o banco
`integration_gate_fixtures`. Ele entra no job `test` do CI por
`cargo test --workspace`. O teste HTTP roda em `scripts/ci/container.sh` com containers
descartáveis. Não usa dados de clientes, providers ou credenciais reais.

**Resultado local:** suíte Rust completa e teste da imagem Docker passaram com
PostgreSQL 16 descartável. O gate comprova a fronteira server-side disponível,
inclusive retry, isolamento e falha de
conexão. Ainda não há, neste ticket, API de `EstadoFinanceiro` persistente,
integração com desktops reais nem teste de provider. Portanto a evidência é
apta para os tickets espelho AutoOS/AutoBO continuarem sua homologação, mas
não autoriza ativação do Produto em Sincronia em staging/produção por si só.

## Repasse para AutoOS e AutoBO

AutoOS deve repetir a jornada usando o contrato v2, conservar `fact_id`,
`event_id`, `idempotency_key` e `correlation_id`, consultar o resultado antes de
reenviar após timeout e comprovar que uma falha do backend não bloqueia a OS.
AutoBO deve consumir o fato por tenant/correlação, validar o contrato de estado
v1 e demonstrar que a transição financeira real não é inferida da ingestão.
Ambos devem registrar os hashes acima e seus próprios resultados no ticket
espelho. `AP-PRODUCT-SYNC-EXT-001` permanece bloqueado até aceite humano dos
dois desktops.
