# AP-INTEGRATION-001 — ingestão do FatoComercial v2

O AutoOS envia o contrato canônico v2 para `POST /v2/commercial-facts` com
`Authorization: Bearer <JWT>` e `x-company-id: <UUID>`. O JWT identifica a
conta; a permissão vem da membership AutoOS ativa dessa conta na empresa,
consultada no PostgreSQL. O `company_id` do corpo deve coincidir com o
header. A API exige `DATABASE_URL` e `JWT_SECRET` em staging e produção.
Em local, sem ambos, expõe somente `/health` e `/ready`.
Com banco configurado, `/ready` verifica conexão, permissão do papel
`integration_app` e a migration das chaves; responde 503 se a ingestão não
estiver operacional.

O primeiro envio retorna HTTP 201 e `{ "fact_id": "...", "created": true }`.
Um retry idêntico retorna HTTP 200 e o mesmo ID. A combinação
`(company_id, os_id, os_version)`, o `fact_id`, o `event_id` e a
`idempotency_key` não podem ser reutilizados com conteúdo diferente:
HTTP 409. Erros de contrato retornam 422; autorização, 401 ou 403.
Em 503 ou resposta perdida, o produtor deve consultar
`GET /v2/commercial-facts/{fact_id}` ou
`GET /v2/commercial-facts?correlation_id=...` antes de reenviar o
mesmo evento. A repetição é segura, inclusive após falha de rede.

O snapshot fica em `fiscal.fact_snapshots`, imutável e protegido por RLS.
As migrations 0006 e 0007 devem ser aplicadas antes de habilitar a rota.
A 0007 completa as chaves de snapshots anteriores e protege inserções feitas
pelo caminho fiscal legado. Se dados antigos tiverem chaves inválidas ou
duplicadas, a migration falha para permitir reconciliação explícita.
O núcleo fiscal lê por ID ou correlação com membership AutoBO FISCAL ativa
pelas funções `find_for_fiscal` e `find_for_fiscal_by_correlation`.
Essa ingestão não cria venda, recebível nem documento fiscal.
O middleware registra identificadores de correlação. Após autorização, a rota
registra o pseudônimo da empresa autenticada no contexto da requisição;
nunca registra o corpo, dados do cliente ou segredos.
