# AP-FISC-CONFIG-001 — cadastro fiscal server-side

O cadastro serve a empresas e estabelecimentos separados. `fiscal.issuer_profiles` guarda município, regime declarado e estado de revisão. `fiscal.item_profiles` guarda o item e a origem da peça (`none`, `customer`, `workshop`); o cadastro do item precisa de revisão FISCAL. `fiscal.operation_scenarios` descreve o tipo (`service`, `goods`, `mixed`), a forma de cobrança (`itemized`, `bundle`), a origem da peça, aprovação, responsável, evidência e vigência. `fiscal.decisions` vincula documentos e valores explícitos a um `fact_id` de OS, cenário, estabelecimento e versão. Decisões aprovadas não são editadas; uma correção usa nova versão.

O resolvedor só aceita uma decisão quando perfil, itens, cenário e decisão estão revisados e vigentes. Confere os tipos dos itens, a forma de cobrança e a origem cadastrada das peças. A própria OS v2 ainda não transporta uma declaração independente de propriedade da peça: a revisão humana precisa conferir esse fato antes de aprovar item e cenário. Não inferir origem da peça ou obrigação de NF-e a partir do campo `kind`, do NCM legado ou de `pricing_mode`. Para preço único, o plano deve conter valores fiscais explícitos e fechar com o total da OS. Falta ou divergência retorna “emissão pendente de revisão”.

O perfil pode ser completado em homologação com empresa e regime **sintéticos**. `AP-FISC-ACCOUNTING-EXT-001` continua aberto para a matriz BMITAG. Suas regras reais só podem ser ativadas após parecer, evidência e vigência; `AP-FISC-PROD-001` retém esse bloqueio e a autorização humana para emissão. Nenhum código fiscal da BMITAG foi preenchido aqui.

## Permissões e segredos

As funções server-side requerem sessão AutoBO com permissão `FISCAL` e executam queries sob `fiscal_app` com RLS por `company_id`. Referências de estabelecimento e cenário têm chaves compostas. `fiscal.provider_configs` armazena apenas DEK encapsulada e credencial cifrada com AES-256-GCM, com contexto autenticado de empresa, estabelecimento, provider e identificador da chave. A chave-mestra `FISCAL_MASTER_KEY_HEX` (64 caracteres hexadecimais) permanece no ambiente do serviço; o desktop e o banco não a recebem. A rotação trava a linha e troca a chave sem expor o segredo em resposta ou log.

## Campos para parecer e parametrização futura

Para cada cenário real, preencher em evidência restrita: empresa/estabelecimento, município/UF, regime e período, CNAEs, item e propriedade da peça, operação, discriminação e valores, documento(s), código municipal de serviço ou CFOP/NCM/CST/CSOSN quando aplicáveis, base, retenções, responsável pela aprovação, fundamento e vigência. O snapshot FatoComercial v2 contém códigos declarados por item, mas eles não substituem a decisão aprovada. Os campos ainda pendentes da BMITAG estão na [pauta para o contador](AP-FISC-ACCOUNTING-EXT-001-matriz-para-contador.md).

## Homologação

Os testes criam duas empresas sintéticas em PostgreSQL descartável. Cobrem rascunho, aprovação, vigência, OS distinta, peça do cliente versus mercadoria da oficina, negação entre tenants e rotação de segredo. Os testes do núcleo também cobrem operação mista, preço único sem valoração, timeout, consulta e idempotência. Adapters e homologação de NFS-e/NF-e permanecem nos respectivos tickets; este cadastro não faz emissão.
