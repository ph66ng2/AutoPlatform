# AP-FISC-ACCOUNTING-EXT-001 — observações do Gerencie Aqui

Consulta somente de leitura à interface da BMITAG em 2026-10-08. Este registro contém apenas configuração e resultados agregados; não reproduz clientes, documentos, XML, credenciais ou dados de notas. A interface mostra a prática cadastrada, não constitui parecer contábil nem prova de que o tratamento fiscal foi aprovado para cada cenário.

## O que foi observado

- A OS separa lançamentos e totais de produtos e serviços. A emissão de NF-e e NFS-e tem telas distintas, e há uma opção para incluir o profissional da OS nas informações complementares da NFS-e. Isso sugere uma relação operacional possível entre OS e nota, mas não comprova o vínculo de uma emissão concreta.
- Há uma tabela de impostos denominada `Simples Nacional` e configurações próprias de NF-e e NFS-e. A NF-e estava configurada em produção. A listagem de 2026-07-01 a 2026-10-08 exibiu NF-e autorizadas com diferentes CFOPs. Esses registros não demonstram como uma OS com peça e preço único foi faturada.
- Na busca do catálogo por manutenção, havia descrições de manutenção com troca de peça e de manutenção com peças cedidas pelo cliente. Um item de manutenção com troca de peça estava marcado como `Produto`, com NCM `0000.00.00`, preço de cadastro zero e tabela NF-e `Simples Nacional`. A classificação operacional do item e esse NCM não podem ser copiados como decisão fiscal aprovada.
- Em uma OS entregue, a descrição era de serviço técnico, mas o valor constava integralmente no total de `Produtos` e o total de `Serviços` era zero. Essa divergência reforça que o tipo do item legado não determina sozinho o documento fiscal.
- A listagem de NFS-e não retornou registros nos períodos consultados: 2025-07-01 a 2025-12-31, 2026-01-01 a 2026-06-30 e 2026-07-01 a 2026-10-08. Isso não prova ausência de emissão: a nota pode estar em outro emissor, empresa ou período.

## Exemplo de NFS-e apresentado pelo responsável

Em 2026-10-13, o responsável apresentou uma NFS-e emitida em setembro de 2026 no portal Nota Salvador. A discriminação descreve manutenção corretiva e preventiva com troca de um componente novo; o documento traz um valor total único, sem preço individual da peça, e classifica a operação como assistência técnica (item municipal 14.02). Isso comprova a forma documental usada nesse exemplo de manutenção com troca de componente, mas a nota não identifica quem era o proprietário ou fornecedor da peça. É compatível com a ausência de resultados na busca feita no Gerencie Aqui. A nota, que contém dados de terceiros, permanece fora deste repositório.

A NFS-e isolada não comprova a OS correspondente nem a validação contábil do enquadramento. Em 2026-10-14, o responsável informou que todas as OS de serviço da BMITAG seguem o padrão de peça do cliente, sem fornecimento ou venda de peça pela BMITAG e com cobrança apenas de mão de obra. Registrar essa premissa operacional na matriz do piloto e confirmar com a OS correspondente à nota. O código 14.02 observado é um dado da emissão; a confirmação contábil posterior, relatada abaixo, aplica-se somente ao cenário descrito da BMITAG, sem extrapolação para outras operações ou empresas.

## OS 63 — orientação escrita da contabilidade (recebida em 2026-10-14)

O responsável confirmou que o texto enviado sobre a OS 63 foi preparado pela contabilidade. Segundo essa orientação, a BMITAG prestou manutenção e reparo, instalou bateria e tampa de propriedade e fornecimento do cliente e cobrou apenas mão de obra; cabe uma NFS-e única pelos serviços, em princípio sob o subitem 14.01, sem NF-e de venda dessas peças pela BMITAG. A autoria contábil foi informada pelo responsável; não foram enviados aqui a OS, a nota correspondente ou dados de identificação do profissional. A NFS-e apresentada acima descreve troca de outro componente; não vinculá-la automaticamente à OS 63.

Esta orientação contábil resolve, para os fatos descritos da OS 63, a dúvida sobre NFS-e única versus NF-e de venda das peças do cliente. Em 2026-10-15, após conferir o código da NFS-e apresentada, o responsável escolheu 14.02 (código municipal observado 1402-0/01) como referência operacional para o piloto. Em 2026-10-18, o responsável confirmou expressamente que a contabilidade também aprovou 14.02 para as OS de serviço da BMITAG com cobrança apenas de mão de obra e, quando há troca, peças pertencentes ao cliente. Essa confirmação posterior esclarece o 14.01 citado "em princípio" na orientação anterior. A confirmação foi transmitida pelo responsável; o teor do parecer atualizado ainda não foi anexado. Antes de ativar emissão, é preciso conferir a OS e o documento correspondente e registrar estabelecimento, base, retenções e vigência aplicáveis. Cenários de peça fornecida pela BMITAG continuam previstos na arquitetura SaaS, mas foram informados como não praticados nas OS de serviço do piloto BMITAG; não exigir uma OS real artificial para validá-los.

## Consequência para o produto

O FatoComercial v2 deve preservar descrição, itens e valores efetivamente cobrados na OS. O núcleo fiscal não deve converter automaticamente um item legado `Produto` em NF-e, nem usar `pricing_mode: bundle` para concluir que cabe apenas NFS-e. `AP-FISC-CORE-001` e `AP-FISC-CONFIG-001` já exigem decisão fiscal fundamentada por empresa, estabelecimento e cenário, e bloqueiam preparação quando faltar regra ou valor necessário. A inspeção do legado reforça esses critérios; não altera o contrato v2 nem autoriza emissão.

## Matriz piloto em preparação

| Cenário | Prática informada | Documento observado/proposto | Decisão ainda necessária |
| --- | --- | --- | --- |
| OS de serviço sem troca de peça | Apenas mão de obra | NFS-e; 14.02 confirmado para as OS de serviço da BMITAG segundo o responsável | Registrar o teor da confirmação contábil, base, retenções e vigência. |
| OS de serviço com peça do cliente | Apenas mão de obra; sem venda da peça pela BMITAG | NFS-e única; 14.02 confirmado pela contabilidade segundo o responsável em 2026-10-18 | Vincular OS e nota; registrar o teor da confirmação contábil, estabelecimento, base, retenções e vigência. |
| OS de serviço com peça fornecida pela BMITAG, com preço único ou itemizada | Informada como não praticada no piloto BMITAG | Não aplicável ao piloto, sujeito à confirmação contábil | Manter fluxo configurável e bloqueado até decisão específica de empresa e cenário; não pedir OS artificial. |
| Venda de mercadoria fora da OS | NF-e observadas no emissor legado | NF-e | Vincular exemplo e confirmar CFOP, NCM, CST/CSOSN, base, estabelecimento e vigência. |

Esta tabela é um rascunho de trabalho. A orientação contábil da OS 63 está registrada, mas não substitui a parametrização completa e aprovada do piloto. Nenhuma linha deve ser ativada como regra de emissão enquanto seus campos necessários permanecerem indefinidos.

## Evidência que ainda falta para concluir o ticket

1. Vincular a OS correspondente à NFS-e já apresentada e identificar o documento da OS 63, se distinto. O responsável informa que as OS de serviço cobram apenas mão de obra e usam, quando há troca, peças do cliente; conferir essa premissa nos exemplos históricos. Localizar os documentos efetivamente emitidos para os cenários praticados, inclusive venda de mercadoria fora da OS se houver. Registrar OS com peça fornecida pela BMITAG, com preço único ou cobrança separada, como não aplicáveis ao piloto enquanto não praticadas, sujeitas à confirmação contábil; não criar uma OS real apenas para produzir evidência. Usar cópias redigidas para revisão; não adicionar documentos reais ou dados pessoais ao repositório público.
2. Consolidar a orientação escrita já recebida da contabilidade para a OS 63 e anexar ou registrar o teor da confirmação posterior de 14.02 (1402-0/01) para o cenário de mão de obra com peças do cliente. Completar as decisões ainda não cobertas para cada cenário praticado e a classificação dos não praticados: CFOP/NCM e CST/CSOSN quando aplicáveis, retenções, bases, estabelecimento e vigência. Se futuramente houver preço único com peça fornecida pela BMITAG, definir critério de valoração sem inventar rateio. Enfrentar expressamente a ressalva de peças/partes do subitem 14.01 da [LC 116/2003](https://www.planalto.gov.br/ccivil_03/leis/lcp/lcp116.htm), distinguindo peça fornecida, material consumido e peça do cliente.
3. Confirmar o fluxo entre a OS e a NFS-e apresentada no portal Nota Salvador, inclusive quem iniciou a emissão e como a referência à OS foi registrada. Registrar limitações por município, regime, estabelecimento e vigência da decisão.

**Estado:** `ready` para preparar e revisar a matriz; aceite completo pendente. A orientação escrita da contabilidade para a OS 63 e a confirmação posterior de 14.02 foram recebidas via responsável, com seus escopos registrados acima. A configuração e o provider aguardam os campos fiscais ainda pendentes; NF-e autorizadas e cadastros legados não substituem esse aceite.
