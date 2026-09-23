# Resultado indeterminado

Alerta: indeterminate_outcome
Owner: plataforma

## Sintoma

A métrica `indeterminate` ficou maior que zero. Timeout financeiro ou fiscal entra aqui.

## Ação

Consultar o estado no provider antes de qualquer nova tentativa. Manter a operação como indeterminada até a consulta responder.

## Não fazer

Não repetir a chamada às cegas. Não marcar pagamento confirmado nem documento emitido.
