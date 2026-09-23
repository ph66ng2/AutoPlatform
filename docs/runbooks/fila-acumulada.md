# Fila acumulada

Alerta: queue_backlog
Owner: plataforma

## Sintoma

A métrica `queue_depth` atingiu o limiar base. Nesta fase o limiar é 1, porque ainda não existe fila de negócio.

## Ação

Ler o trace pelo `correlation_id` e ver qual worker publicou a profundidade. Quando a fila real existir, o owner revisa o limiar neste runbook e no código.

## Não fazer

Não drenar fila apagando evento. Não registrar o payload do item.
