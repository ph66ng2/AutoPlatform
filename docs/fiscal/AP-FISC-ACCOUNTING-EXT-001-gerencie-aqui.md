# AP-FISC-ACCOUNTING-EXT-001 — observações do Gerencie Aqui

Consulta somente de leitura à interface da BMITAG em 2026-10-08. Este registro contém apenas configuração e resultados agregados; não reproduz clientes, documentos, XML, credenciais ou dados de notas. A interface mostra a prática cadastrada, não constitui parecer contábil nem prova de que o tratamento fiscal foi aprovado para cada cenário.

## O que foi observado

- A OS separa lançamentos e totais de produtos e serviços. A emissão de NF-e e NFS-e tem telas distintas, e há uma opção para incluir o profissional da OS nas informações complementares da NFS-e. Isso sugere uma relação operacional possível entre OS e nota, mas não comprova o vínculo de uma emissão concreta.
- Há uma tabela de impostos denominada `Simples Nacional` e configurações próprias de NF-e e NFS-e. A NF-e estava configurada em produção. A listagem de 2026-07-01 a 2026-10-08 exibiu NF-e autorizadas com diferentes CFOPs. Esses registros não demonstram como uma OS com peça e preço único foi faturada.
- Na busca do catálogo por manutenção, havia descrições de manutenção com troca de peça e de manutenção com peças cedidas pelo cliente. Um item de manutenção com troca de peça estava marcado como `Produto`, com NCM `0000.00.00`, preço de cadastro zero e tabela NF-e `Simples Nacional`. A classificação operacional do item e esse NCM não podem ser copiados como decisão fiscal aprovada.
- Em uma OS entregue, a descrição era de serviço técnico, mas o valor constava integralmente no total de `Produtos` e o total de `Serviços` era zero. Essa divergência reforça que o tipo do item legado não determina sozinho o documento fiscal.
- A listagem de NFS-e não retornou registros nos períodos consultados: 2025-07-01 a 2025-12-31, 2026-01-01 a 2026-06-30 e 2026-07-01 a 2026-10-08. Isso não prova ausência de emissão: a nota pode estar em outro emissor, empresa ou período.

## Exemplo de NFS-e apresentado pelo responsável

Em 2026-10-13, o responsável apresentou uma NFS-e emitida em setembro de 2026 no portal Nota Salvador. A discriminação descreve manutenção corretiva e preventiva com troca de um componente novo; o documento traz um valor total único, sem preço individual da peça, e classifica a operação como assistência técnica (item municipal 14.02). Isso comprova a forma documental usada nesse exemplo de serviço com peça incluída no preço único e é compatível com a ausência de resultados na busca feita no Gerencie Aqui. A nota, que contém dados de terceiros, permanece fora deste repositório.

A NFS-e isolada não comprova a OS correspondente, eventual NF-e ligada à peça nem a validação contábil do enquadramento. O código 14.02 observado é um dado da emissão; cabe ao contador confirmar seu fundamento e a aplicação da ressalva relativa a peças/partes do subitem 14.01 da LC 116/2003, sem extrapolar a regra para outras operações ou empresas.

## Consequência para o produto

O FatoComercial v2 deve preservar descrição, itens e valores efetivamente cobrados na OS. O núcleo fiscal não deve converter automaticamente um item legado `Produto` em NF-e, nem usar `pricing_mode: bundle` para concluir que cabe apenas NFS-e. `AP-FISC-CORE-001` e `AP-FISC-CONFIG-001` já exigem decisão fiscal fundamentada por empresa, estabelecimento e cenário, e bloqueiam preparação quando faltar regra ou valor necessário. A inspeção do legado reforça esses critérios; não altera o contrato v2 nem autoriza emissão.

## Evidência que ainda falta para desbloquear

1. Identificar exemplos históricos dos cenários distintos praticados pela BMITAG: OS de manutenção sem peça; OS com peça fornecida e preço único; OS com serviço e peça cobrados em linhas separadas; e venda de mercadoria fora da OS. A NFS-e de preço único foi apresentada; ainda falta vincular a OS e verificar se houve outro documento fiscal para a peça. Para os demais cenários praticados, localizar os documentos efetivamente emitidos e o vínculo com a OS quando houver. Se não existir OS com cobrança separada, registrar esse cenário como não aplicável ao piloto, com confirmação do contador; não criar uma OS real apenas para produzir evidência. Usar cópias redigidas para revisão; não adicionar documentos reais ou dados pessoais ao repositório público.
2. Obter a decisão escrita do contador para cada cenário praticado e a classificação explícita dos não praticados: enquadramento, NFS-e/NF-e exigida(s), código municipal de serviço, CFOP/NCM e CST/CSOSN quando aplicáveis, retenções, bases e critério de valoração se o preço único abranger serviço e peça. Enfrentar expressamente a ressalva de peças/partes do subitem 14.01 da [LC 116/2003](https://www.planalto.gov.br/ccivil_03/leis/lcp/lcp116.htm), distinguindo peça fornecida, material consumido e peça do cliente.
3. Confirmar o fluxo entre a OS e a NFS-e apresentada no portal Nota Salvador, inclusive quem iniciou a emissão e como a referência à OS foi registrada. Registrar limitações por município, regime, estabelecimento e vigência da decisão.

**Estado:** `blocked`. A aprovação relatada em conversa para NFS-e única deve ser vinculada ao cenário e ao fundamento escrito do contador antes de liberar configuração e provider. A existência de NF-e autorizadas ou de um cadastro legado não satisfaz esse aceite.
