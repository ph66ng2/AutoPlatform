# ADR 0001 — Billing da plataforma e pagamento da oficina

Status: aceita neste ticket
Owner: plataforma

## Contexto

A assinatura do SaaS e a cobrança da oficina podem usar o mesmo tipo de provider, mas não são o mesmo dinheiro, o mesmo webhook, a mesma idempotência nem o mesmo OAuth.

## Decisão

1. Os contextos `platform_billing` e `workshop_payments` ficam separados. Credencial, webhook, `idempotency_key` e reconciliação não se misturam.
2. Nenhum provider foi escolhido. Asaas e Mercado Pago só entram no adapter final depois de score live ≥ 80 e de todos os bloqueadores no contexto correspondente.
3. Redirect de checkout não confirma pagamento. Timeout financeiro vira indeterminado e exige consulta antes de qualquer retry.
4. Segredo de provider não entra no desktop. O desktop só vê se a oficina autorizou.

## Por que não escolher agora

A matriz live não rodou: não há conta sandbox neste repositório e este ticket não cobra cliente. Escolher Asaas ou Mercado Pago aqui seria preferência sem prova.

## Consequências

- AP-BILL e AP-PAY evoluem em frentes distintas, mesmo que no futuro o vendor coincida.
- O spike em `spikes/gateway` é evidência do protocolo, não o adapter.
