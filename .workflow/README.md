# Workflow do AutoPlatform

O catálogo local é a fonte de verdade para tickets, estados e dependências.

**Foco atual:** [AutoOS Fiscal PROD BMITAG](FISCAL_PROD_ROADMAP.md). AutoPlatform fornece contrato v2, ingestão e backend Fiscal; Financeiro básico vem depois. O campo `roadmap` registra prioridade, adiamentos e substituições separadamente de `status` e `blockedBy`.

## Comandos

```bash
.workflow/scripts/waves.sh plan .workflow/workflow.json
.workflow/scripts/waves.sh spawn .workflow/workflow.json AP-GOV-001
```

Um ticket somente está liberado quando tem status `ready` e todos os itens de `blockedBy` estão `merged`. Tickets `blocked` representam decisão ou entrega externa e ficam sem onda.

## Marcos

1. Fundação e identidade já entregues; reconciliar `AP-INV-001` com o PR #9 sem presumir aceite automático.
2. `AP-CONTRACT-002` e matriz contábil `AP-FISC-ACCOUNTING-EXT-001`; contrato pode avançar enquanto a matriz é obtida.
3. Em paralelo: `AP-INTEGRATION-001` ingere o fato do AutoOS e `AP-FISC-CORE-001` cria núcleo desacoplado de Sale/estoque central.
4. Configuração fiscal; NFS-e e NF-e em adapters separados; gate backend de homologação.
5. Gate integrado AutoOS e piloto controlado `AP-FISC-PROD-001` com aprovação humana.
6. Financeiro v1; depois SaaS, pagamentos, Portal, offline e migração conforme nova decisão.

Como existe uma frente principal, ondas expressam dependências, não autorização para iniciar tudo em paralelo. Priorize um ticket de implementação por vez, salvo pedido humano explícito.

Os planejadores ainda não consomem `externalPrerequisites`; não trate esses campos `proposed` como arestas confirmadas. `AP-FISC-PROD-001` fica `blocked` até confirmação externa do gate AutoOS e autorização do piloto.

## Tickets externos

- `AP-FISC-ACCOUNTING-EXT-001`: matriz fiscal validada pela contabilidade.
- `AP-CLIENTS-PORTAL-EXT-001`: AutoOS e AutoBO consumindo o Portal em staging.
- `AP-PRODUCT-SYNC-EXT-001`: gates de Produto em Sincronia homologados nos dois desktops.

Esses tickets não implementam código neste repositório. Só podem virar `merged` com evidência e confirmação humana.
