#!/bin/bash

MYSQL_HOST="192.168.123.199" 
MYSQL_PORT=3306
MYSQL_USER="root"
MYSQL_PASS="admin!"
MYSQL_DB="ha_test"

total_ok=0
total_fail=0
total_read_fail=0
fail_start=""
fail_count=0

while true; do
  mysql -h "${MYSQL_HOST}" -P "${MYSQL_PORT}" \
        -u "${MYSQL_USER}" -p"${MYSQL_PASS}" \
        "${MYSQL_DB}" \
        -e "INSERT INTO messages (hostname, message) VALUES (@@hostname, CONCAT('HA test at ', NOW()));" \
        >/dev/null 2>&1

  write_status=$?

  if [ $write_status -eq 0 ]; then
    total_ok=$((total_ok+1))
    last_row=$(mysql -h "${MYSQL_HOST}" -P "${MYSQL_PORT}" \
                     -u "${MYSQL_USER}" -p"${MYSQL_PASS}" \
                     -D "${MYSQL_DB}" \
                     -e "SELECT id, hostname, message, created_at FROM messages ORDER BY id DESC LIMIT 1;" 2>/dev/null)

    if [ $? -ne 0 ]; then
      total_read_fail=$((total_read_fail+1))
      echo "$(date '+%F %T')  READ FAILED"
    else
      echo "$(date '+%F %T')  WRITE OK | READ OK | Last row: $(echo "$last_row" | tail -n1)"
    fi
  else
    total_fail=$((total_fail+1))
    if [ -z "$fail_start" ]; then
      fail_start=$(date +%s)
      fail_count=1
      echo "$(date '+%F %T')  FAILOVER STARTED"
    else
      fail_count=$((fail_count+1))
    fi
  fi

  if [ ! -z "$fail_start" ] && [ $write_status -eq 0 ]; then
    fail_end=$(date +%s)
    duration=$((fail_end - fail_start))
    echo "$(date '+%F %T')  FAILOVER ENDED | duration: ${duration}s | failed writes: $fail_count | total read fails: $total_read_fail"
    fail_start=""
    fail_count=0
    total_read_fail=0
  fi

  sleep 1
done
