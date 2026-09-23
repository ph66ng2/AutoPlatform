# Retries esgotados

Alerta: retry_exhausted
Owner: plataforma

## Sintoma

A métrica `retries` atingiu 3.

## Ação

Parar o ciclo. Abrir o trace pelo `correlation_id` e separar falha definitiva de resultado indeterminado. Indeterminado segue o runbook de consulta.

## Não fazer

Não aumentar o retry no escuro. Não logar o corpo da tentativa.
