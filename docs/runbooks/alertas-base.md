# Alertas base

Owner: plataforma

Estes alertas existem antes de fila, provider e nota fiscal reais. O disparo é em processo, com trace sintético. Nenhum coletor externo foi criado.

| Alerta | Runbook |
| --- | --- |
| `provider_failure` | [falha-de-provider.md](falha-de-provider.md) |
| `indeterminate_outcome` | [resultado-indeterminado.md](resultado-indeterminado.md) |
| `queue_backlog` | [fila-acumulada.md](fila-acumulada.md) |
| `retry_exhausted` | [retries-esgotados.md](retries-esgotados.md) |

O log leva `correlation_id`, `event_id` e o pseudônimo do tenant. Não leva XML, documento, token nem segredo.
