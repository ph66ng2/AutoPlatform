# AutoOS Fiscal PROD BMITAG — plano de execução

**Decisão de produto:** AutoOS mantém a operação e a interface fiscal; AutoPlatform fornece contrato, ingestão e emissão server-side; AutoBO permanece como fonte de código, regras e histórico. O primeiro marco é uma OS real da BMITAG produzir NFS-e e NF-e nos cenários aplicáveis, com retorno e rastreabilidade. Financeiro básico vem depois. SaaS, billing de assinatura, PowerSync, Portal e integração entre dois desktops deixam de ser pré-requisitos do Fiscal.

Este documento orienta a **prioridade**. Os três `.workflow/workflow.json` continuam sendo a fonte de status e `blockedBy` local. O objeto `roadmap` em cada catálogo registra foco, adiamentos e substituições sem converter ticket adiado em `blocked` ou `merged`. Os planejadores atuais ignoram `roadmap` e `externalPrerequisites`; uma onda tecnicamente liberada não é autorização de prioridade nem prova de dependência cruzada satisfeita.

**Revisão coordenada:** [AutoOs #102](https://github.com/ph66ng2/AutoOs/pull/102) · [AutoPlatform #10](https://github.com/ph66ng2/AutoPlatform/pull/10) · [AutoBO #9](https://github.com/ph66ng2/AutoBO/pull/9). Os três PRs estão em rascunho e nenhum deles implementa o fluxo fiscal.

## Primeiro movimento: quatro frentes em paralelo

| Frente | Ticket | Entrega de aceite |
|---|---|---|
| Governança | `AO-WF-RECONCILE-001` | Revisar os três PRs de roadmap e registrar itens ativos, adiados e substituídos. |
| Ownership | `AO-SUITE-001` | AutoOS é autoridade de cliente, produto, serviço e estoque; equipamento e OS pertencem à operação. |
| Contrato | `AP-CONTRACT-002` | FatoComercial v2 versionado, imutável após ingestão, validado com OS de serviço, produto e mista; v1 compatível. |
| Condição externa | `AP-FISC-ACCOUNTING-EXT-001` | Matriz fiscal BMITAG validada pela contabilidade; permanece `blocked` até confirmação humana. |

`AP-INV-001` possui [PR #9 mergeado](https://github.com/ph66ng2/AutoPlatform/pull/9) e ainda aparece `in_progress` no catálogo. Conciliar evidência e aceite antes de mudar o status. Estoque central e `AP-SALE-001` não bloqueiam o núcleo Fiscal.

## Fluxo até o piloto

1. **Fato e ingestão:** `AO-SUITE-002` produz o snapshot de OS no v2 após ownership e contrato aprovados. `AP-INTEGRATION-001` ingere e persiste por empresa, OS e versão, com resposta idempotente. `AP-FISC-CORE-001` aceita o fato sem exigir Sale ou estoque central. Produtor, ingestão e núcleo podem ser construídos em paralelo sobre exemplos versionados; a integração real exige os três compatíveis.
2. **Emissão e UI em paralelo:** `AP-FISC-CONFIG-001` exige núcleo e matriz contábil. `AP-FISC-NFSE-001` e `AP-FISC-NFE-001` continuam adapters separados. No AutoOS, `AO-FISC-001` cria a fila, `AO-FISC-002` a revisão, `AO-FISC-003` a API e `AO-FISC-004/005/006` cobrem estados, documentos e histórico. A UI pode avançar com fake contratual enquanto adapters são homologados.
3. **Gates separados:** `AP-FISC-GATE-001` prova o backend em homologação. `AO-FISC-GATE-001` prova orçamento → OS → fato → revisão → emissão → retorno → documento → histórico. Nenhum gate executa piloto real. `AP-FISC-PROD-001` permanece `blocked` até os dois gates, matriz contábil, autorização humana e condições reais do piloto.

`blockedBy` só contém IDs do próprio repositório. `externalPrerequisites` nos novos tickets torna a condição externa visível, com estado `proposed`, mas os scripts não a aplicam. Para reger elegibilidade automática, aprovar um contrato versionado de arestas cruzadas e atualizar os planejadores primeiro. Até lá, os gates externos permanecem `blocked` até confirmação humana.

## Critérios do marco Fiscal PROD

O piloto controlado BMITAG registra, com acesso restrito e evidência redigida:

- pelo menos uma NFS-e e uma NF-e reais no conjunto de cenários aprovados;
- consulta real, XML persistido e documento auxiliar recuperável;
- vínculo inequívoco entre documento, fato v2 e OS;
- rejeição tratável, falha parcial visível e timeout consultado antes de retry;
- idempotência demonstrada sem emissão duplicada;
- cancelamento validado no ambiente apropriado antes de ser declarado coberto;
- Go/No-Go de operação, Fiscal e contabilidade.

Uma OS individual pode exigir apenas um dos tipos. O **marco** exige evidência de ambos no conjunto do piloto. Se a decisão for lançar um tipo primeiro, dividir o marco em dois gates por revisão. Este PR de planejamento não autoriza emissão real.

## Depois do marco

Financeiro v1 é a próxima **prioridade**, não um bloqueador técnico inventado para Fiscal: `AP-REC-001` (contas a receber), `AP-REC-002` (baixa manual) e `AO-FIN-001` (consulta no AutoOS). Criar especificações desses tickets depois de estabilizar contrato e piloto. Só então retomar SaaS, billing, entitlements, onboarding, PowerSync/offline, Field e, mais tarde, Sicredi/boleto/Pix.

AutoBO preserva histórico e mapeia substituições em `roadmap.superseded`. Não apagar tickets nem marcá-los `merged` para esconder trabalho não feito. `AP-MIG-001` precisa de decisão própria: confirmar se o piloto depende de dados históricos antes de classificá-lo. IDs adiados constam em `roadmap.deferredIds` de cada projeto.

## Revisão e entrada em execução

1. Revisar os três PRs como um conjunto; conferir que nenhum ticket foi marcado `merged` pela mudança de prioridade.
2. Confirmar ownership e contrato v2; decidir arestas externas e adaptar planejadores antes de usá-las como bloqueadores automáticos.
3. Mesclar por decisão humana em `AutoOs@feature`, `AutoPlatform@main` e `AutoBO@main`; reler os três workflows e recalcular ondas.
4. Iniciar tickets elegíveis do foco Fiscal conforme cada especificação. O merge do planejamento não implementa contrato, API, UI nem emissão.
