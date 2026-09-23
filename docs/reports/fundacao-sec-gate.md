# Fundação — isolamento e segredos

Owner: plataforma
Ticket: AP-SEC-GATE-001
Veredito: pendente de revisão humana

Este relatório não libera produção. O merge humano deste PR aceita os riscos residuais para avançar implementação de billing e estoque. Go/No-Go de produção fica para o marco de release.

## Matriz executada

| Ameaça | Resultado | Evidência |
| --- | --- | --- |
| Tenant A lê ou escreve B | recusado | `tests/security/tests/abuse.rs`, `crates/identity/tests/tenant_isolation.rs` |
| `company_id` no JWT | ignorado | `tests/security/tests/matrix.rs` |
| ADMIN AutoOS vira FISCAL AutoBO | recusado | `tests/security/tests/matrix.rs` |
| Membership suspensa ou revogada | recusado | `crates/identity/tests/tenant_isolation.rs` |
| XML, token ou documento em log | redigido | `tests/security/tests/matrix.rs` |
| Versão de contrato desconhecida | recusada | `tests/security/tests/matrix.rs` |
| Papel de serviço e chave-mestra no cliente | ausente | varredura de `examples/`, `openapi/`, `crates/portal/` |
| Segredo no `.env.example` ou Dockerfile | ausente | `tests/security/tests/matrix.rs` |
| Migration no startup | recusada | processo e `infra/environments/` |
| identity_app com BYPASSRLS | recusado | `tests/security/tests/abuse.rs` |

A evidência acima cita arquivo e teste. Nenhum dump de banco, token ou XML entra neste relatório.

## Riscos residuais

- Staging e produção ainda não foram provisionados. O ensaio usa PostgreSQL descartável de CI.
- Não houve pentest externo.
- Environments GitHub `staging` e `production` ainda dependem de revisor humano.
- Tokens de provider ainda não existem neste repositório.

## Não fazer

Não colar segredo, XML ou documento na evidência. Não promover este veredito a Go de produção.
