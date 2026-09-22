# AutoPlatform

Backend compartilhado do AutoOS e AutoBO.

O AutoPlatform será um monólito modular em Rust, implantado como API HTTP e worker/reconciliador a partir da mesma base de código. Ele concentra identidade empresarial, assinaturas, entitlements, contratos, estoque central, vendas, pagamentos, fiscal, Sicredi, integração, Portal, auditoria e observabilidade.

Este repositório foi iniciado apenas com governança e planejamento. A implementação começa pelo ticket `AP-GOV-001`; nenhum serviço ou ambiente de produção foi criado no bootstrap.

## Regras fundamentais

- AutoOS e AutoBO são produtos separados e funcionam de forma independente.
- O mesmo `company_id` e a mesma identidade atendem os dois produtos.
- Entitlements são concedidos por produto e capacidade.
- Toda tabela de negócio possui `company_id`; tabelas expostas usam RLS.
- Desktop usa somente chave publicável e JWT.
- Segredos bancários, fiscais, OAuth e `service_role` permanecem server-side.
- Eventos usam outbox transacional, inbox idempotente e contratos versionados.
- Produção nunca executa migrations automaticamente no startup.
- Nenhum ticket faz merge, deploy, cobrança, emissão fiscal ou migração de produção automaticamente.

## Workflow

A fonte de verdade é [`.workflow/workflow.json`](.workflow/workflow.json).

```bash
.workflow/scripts/waves.sh plan .workflow/workflow.json
.workflow/scripts/waves.sh spawn .workflow/workflow.json AP-GOV-001
```

