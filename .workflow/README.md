# Workflow do AutoPlatform

O catálogo local é a fonte de verdade para tickets, estados e dependências.

## Comandos

```bash
.workflow/scripts/waves.sh plan .workflow/workflow.json
.workflow/scripts/waves.sh spawn .workflow/workflow.json AP-GOV-001
```

Um ticket somente está liberado quando tem status `ready` e todos os itens de `blockedBy` estão `merged`. Tickets `blocked` representam decisão ou entrega externa e ficam sem onda.

## Marcos

1. Fundação: repositório, Rust, CI, ambientes e observabilidade.
2. Identidade e contratos compartilhados.
3. Billing e ativação SaaS.
4. Estoque e vendas standalone.
5. Fiscal prioritário.
6. Pagamentos e Sicredi.
7. Produto em Sincronia.
8. Cutover de estoque e Portal Pagável.
9. PowerSync, migração BMITAG e disponibilidade geral.

Como existe uma frente principal, ondas expressam dependências, não autorização para iniciar tudo em paralelo. Priorize um ticket de implementação por vez, salvo pedido humano explícito.

## Tickets externos

- `AP-FISC-ACCOUNTING-EXT-001`: matriz fiscal validada pela contabilidade.
- `AP-CLIENTS-PORTAL-EXT-001`: AutoOS e AutoBO consumindo o Portal em staging.
- `AP-PRODUCT-SYNC-EXT-001`: gates de Produto em Sincronia homologados nos dois desktops.

Esses tickets não implementam código neste repositório. Só podem virar `merged` com evidência e confirmação humana.

