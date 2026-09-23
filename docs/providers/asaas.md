# Asaas

Contexto: candidato a billing da plataforma e a pagamento da oficina. Os dois fluxos não compartilham conta, webhook nem idempotency_key.

## Bloqueadores

Prova sintética no spike: webhook com token de header, idempotência, timeout que exige consulta, redirect que não confirma, OAuth só no servidor.

Prova live de sandbox: não executada. Sem credencial no Git e sem cobrança.

## Gaps

Pix, cartão, refund, chargeback, recorrência e reconciliação no sandbox Asaas ficam para quando um humano gravar credenciais fora do repositório. Sem essa prova o provider não é escolhido.
