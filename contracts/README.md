# Contratos canônicos v1

Estes JSON Schemas são o contrato compartilhado. AutoOS e AutoBO comparam o `manifest.sha256`. Não copiam o schema para um formato paralelo.

Compatível em v1: campo opcional novo. Quebra: remover campo, mudar tipo, acrescentar required, tirar valor de enum. Quebra exige `contracts/v2/`.

`schema_version` diferente de `1` é rejeitada sem efeito parcial. Todo contrato leva `company_id` e `correlation_id`. Contratos de mutação levam `idempotency_key`.

Campos não mapeiam tabela interna. XML, documento da pessoa, token e segredo não entram no schema.
