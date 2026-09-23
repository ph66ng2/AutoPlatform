# Falha de provider

Alerta: provider_failure
Owner: plataforma

## Sintoma

A métrica `provider_failures` ficou maior que zero.

## Ação

Abrir o trace pelo `correlation_id`. Anotar o nome do provider e o resultado. O ticket de acompanhamento cita o id do alerta e o runbook, sem corpo de resposta.

## Não fazer

Não colar token, documento, XML ou segredo no chamado. Não transformar a falha em sucesso.
