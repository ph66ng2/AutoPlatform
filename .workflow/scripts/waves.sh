#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf 'Uso: %s {plan|spawn} .workflow/workflow.json [TICKET-ID]\n' "${0##*/}" >&2
  exit 64
}

[[ $# -ge 2 ]] || usage
command="$1"
workflow="$2"
[[ -f "$workflow" ]] || { printf 'Arquivo não encontrado: %s\n' "$workflow" >&2; exit 66; }
command -v jq >/dev/null || { printf 'Dependência ausente: jq\n' >&2; exit 69; }

jq -e '
  def strings: type == "array" and length > 0 and all(.[]; type == "string" and length > 0);
  (.project | type == "string" and length > 0) and
  (.baseBranch == "origin/main") and
  (.tickets | type == "array") and
  (([.tickets[].id] | length) == ([.tickets[].id] | unique | length)) and
  all(.tickets[];
    (.id | type == "string" and length > 0) and
    (.title | type == "string" and length > 0) and
    (.status | IN("ready", "in_progress", "review", "merged", "blocked")) and
    (.blockedBy | type == "array") and
    (.context | type == "string" and length > 0) and
    (.objective | type == "string" and length > 0) and
    (.scope | strings) and (.outOfScope | strings) and
    (.expectedBehavior | type == "string" and length > 0) and
    (.acceptanceCriteria | strings) and (.tests | strings) and
    (.testInstructions | type == "object") and
    (.testInstructions.prerequisites | strings) and
    (.testInstructions.steps | strings) and
    (.testInstructions.expectedResultAndEvidence | strings) and
    (.testInstructions.dataImpact | type == "string" and length > 0) and
    (.testInstructions.cleanupAndRollback | strings) and
    (.testInstructions.stagingRestrictions | type == "string" and length > 0) and
    (.likelyFiles | strings) and (.risks | strings)
  )
' "$workflow" >/dev/null || { printf 'workflow.json inválido ou ticket incompleto.\n' >&2; exit 65; }

jq -e '.tickets as $t | all($t[]; all(.blockedBy[]?; . as $d | any($t[]; .id == $d)))' "$workflow" >/dev/null || {
  printf 'workflow.json possui dependência ausente.\n' >&2
  exit 65
}

cycle_count=$(jq -r '
  [.tickets[].id] as $ids |
  reduce .tickets[] as $t ({}; .[$t.id] = $t.blockedBy) |
  . as $deps |
  def visit($id; $path):
    if ($path | index($id)) then 1
    else reduce ($deps[$id][]?) as $d (0; . + visit($d; $path + [$id])) end;
  reduce $ids[] as $id (0; . + visit($id; []))
' "$workflow")
[[ "$cycle_count" == "0" ]] || { printf 'Ciclo de dependências detectado.\n' >&2; exit 65; }

case "$command" in
  plan)
    jq -r '
      .tickets as $all |
      $all[] | select(.status != "merged") | . as $ticket |
      [.blockedBy[]? as $d | ($all[] | select(.id == $d) | .status)] as $states |
      if .status != "ready" then
        "EM_\(.status | ascii_upcase)\t\(.id)\t\(.title)"
      elif ($states | length == 0 or all(.[]; . == "merged")) then
        "LIBERADO\t\(.id)\t\(.title)"
      else
        "BLOQUEADO\t\(.id)\t\(.title)\tpor: \(.blockedBy | join(", "))"
      end
    ' "$workflow"
    ;;
  spawn)
    [[ $# -eq 3 ]] || usage
    id="$3"
    ticket=$(jq -ce --arg id "$id" '.tickets[] | select(.id == $id)' "$workflow") || { printf 'Ticket inexistente: %s\n' "$id" >&2; exit 65; }
    [[ "$(jq -r '.status' <<<"$ticket")" == "ready" ]] || { printf 'Ticket %s não está ready.\n' "$id" >&2; exit 65; }
    ok=$(jq -r --arg id "$id" '.tickets as $a | ($a[]|select(.id==$id)|.blockedBy) as $d | all($d[]?; . as $x | ($a[]|select(.id==$x)|.status)=="merged")' "$workflow")
    [[ "$ok" == "true" ]] || { printf 'Ticket %s ainda está bloqueado.\n' "$id" >&2; exit 65; }
    git fetch origin main
    root=$(git rev-parse --show-toplevel)
    safe=$(tr '[:upper:]' '[:lower:]' <<<"$id" | tr -cs 'a-z0-9' '-')
    branch="agent/$safe"
    target="$root/../$(basename "$root")-$safe"
    git worktree add -b "$branch" "$target" origin/main
    printf 'Worktree: %s\nBranch: %s\n' "$target" "$branch"
    ;;
  *) usage ;;
esac
