# Mercado Pago

Contexto: candidato a billing da plataforma e a pagamento da oficina. Os dois fluxos não compartilham conta, webhook nem idempotency_key.

## Bloqueadores

Prova sintética no spike: assinatura `ts`/`v1` HMAC-SHA256, idempotência, timeout que exige consulta, redirect que não confirma, OAuth só no servidor.

Prova live de sandbox: não executada. Sem credencial no Git e sem cobrança.

## Gaps

Pix, cartão, refund, chargeback, recorrência, OAuth de oficina e reconciliação no sandbox Mercado Pago ficam para quando um humano gravar credenciais fora do repositório. Sem essa prova o provider não é escolhido.
