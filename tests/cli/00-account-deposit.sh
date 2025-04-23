#!/bin/bash

RECALL_CLI=${RECALL_CLI:-"recall"}

get_account_info() {
  ${RECALL_CLI} account info
}

extract_balances() {
  local info="$1"
  local balance
  balance=$(echo "$info" | jq -r '.balance')
  local parent_balance
  parent_balance=$(echo "$info" | jq -r '.parent_balance')
  echo "$balance $parent_balance"
}

show_account_info() {
  echo "$1" | jq '.'
}

# Get initial balances and deposit
init_info=$(get_account_info)
read -r init_balance init_parent_balance < <(extract_balances "$init_info")

${RECALL_CLI} account deposit 1

# Wait for balances to update
for i in {1..120}; do
  info=$(get_account_info)
  read -r balance parent_balance < <(extract_balances "$info")
  
  if [ "$(echo "$balance > $init_balance" | bc -l)" -eq 1 ] && \
     [ "$(echo "$parent_balance < $init_parent_balance" | bc -l)" -eq 1 ]; then
    echo "Deposit successful after ${i}s:"
    show_account_info "$info"
    exit 0
  fi
  
  echo -n "."
  sleep 1
done

echo -e "\nTimeout waiting for deposit to complete"
final_info=$(get_account_info)
show_account_info "$final_info"
exit 1
