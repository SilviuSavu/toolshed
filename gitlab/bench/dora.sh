#!/usr/bin/env bash
set -euo pipefail

# GitLab DORA Metrics Calculator
# Computes the 4 DORA metrics from GitLab CE API data.
#
# Usage: GITLAB_TOKEN=glpat-xxx ./dora.sh [days]
#
# Metrics:
#   1. Deployment Frequency  — successful main-branch pipelines per week
#   2. Lead Time for Changes — median time from first commit to pipeline finish
#   3. Change Failure Rate   — failed / total main-branch pipelines
#   4. Time to Restore       — median duration of failures (failed → next success)

GITLAB_URL="${GITLAB_URL:-http://localhost:80}"
GITLAB_TOKEN="${GITLAB_TOKEN:?Set GITLAB_TOKEN}"
DAYS="${1:-30}"
SINCE=$(date -u -v-${DAYS}d +%Y-%m-%dT00:00:00Z 2>/dev/null || date -u -d "-${DAYS} days" +%Y-%m-%dT00:00:00Z)

api() {
  curl -sf "${GITLAB_URL}/api/v4${1}" -H "PRIVATE-TOKEN: ${GITLAB_TOKEN}"
}

echo "============================================================"
echo "  GitLab DORA Metrics (last ${DAYS} days)"
echo "  Since: ${SINCE}"
echo "============================================================"
echo

# Get all projects
PROJECTS=$(api "/projects?per_page=100&simple=true" | jq -r '.[] | "\(.id)|\(.path_with_namespace)"')

TOTAL_DEPLOYMENTS=0
TOTAL_FAILURES=0
TOTAL_PIPELINES=0
ALL_LEAD_TIMES=""
ALL_RESTORE_TIMES=""

while IFS='|' read -r PID PNAME; do
  # Get main-branch pipelines since $SINCE
  PIPELINES=$(api "/projects/${PID}/pipelines?ref=main&per_page=100&updated_after=${SINCE}" | jq -c '.[]')

  if [ -z "$PIPELINES" ]; then
    continue
  fi

  SUCCESS=0
  FAILED=0
  PIPE_COUNT=0
  LAST_FAIL_END=""

  while IFS= read -r P; do
    STATUS=$(echo "$P" | jq -r '.status')
    UPDATED=$(echo "$P" | jq -r '.updated_at')
    CREATED=$(echo "$P" | jq -r '.created_at')
    PIPE_COUNT=$((PIPE_COUNT + 1))

    if [ "$STATUS" = "success" ]; then
      SUCCESS=$((SUCCESS + 1))

      # Lead time: created_at → updated_at (pipeline duration as proxy)
      CREATED_TS=$(date -j -f "%Y-%m-%dT%H:%M:%S" "${CREATED%%.*}" +%s 2>/dev/null || date -d "${CREATED}" +%s)
      UPDATED_TS=$(date -j -f "%Y-%m-%dT%H:%M:%S" "${UPDATED%%.*}" +%s 2>/dev/null || date -d "${UPDATED}" +%s)
      LEAD=$((UPDATED_TS - CREATED_TS))
      ALL_LEAD_TIMES="${ALL_LEAD_TIMES} ${LEAD}"

      # If we had a prior failure, compute restore time
      if [ -n "$LAST_FAIL_END" ]; then
        RESTORE=$((UPDATED_TS - LAST_FAIL_END))
        if [ "$RESTORE" -gt 0 ]; then
          ALL_RESTORE_TIMES="${ALL_RESTORE_TIMES} ${RESTORE}"
        fi
        LAST_FAIL_END=""
      fi
    elif [ "$STATUS" = "failed" ]; then
      FAILED=$((FAILED + 1))
      FAIL_TS=$(date -j -f "%Y-%m-%dT%H:%M:%S" "${UPDATED%%.*}" +%s 2>/dev/null || date -d "${UPDATED}" +%s)
      LAST_FAIL_END="$FAIL_TS"
    fi
  done <<< "$PIPELINES"

  TOTAL_DEPLOYMENTS=$((TOTAL_DEPLOYMENTS + SUCCESS))
  TOTAL_FAILURES=$((TOTAL_FAILURES + FAILED))
  TOTAL_PIPELINES=$((TOTAL_PIPELINES + PIPE_COUNT))

  if [ "$PIPE_COUNT" -gt 0 ]; then
    CFR=0
    if [ "$PIPE_COUNT" -gt 0 ]; then
      CFR=$(echo "scale=1; $FAILED * 100 / $PIPE_COUNT" | bc)
    fi
    echo "  ${PNAME}: ${SUCCESS} deploys, ${FAILED} failed, ${PIPE_COUNT} total (CFR: ${CFR}%)"
  fi
done <<< "$PROJECTS"

echo
echo "------------------------------------------------------------"
echo "  AGGREGATE METRICS"
echo "------------------------------------------------------------"

# 1. Deployment Frequency
WEEKS=$(echo "scale=1; $DAYS / 7" | bc)
if [ "$(echo "$WEEKS > 0" | bc)" -eq 1 ]; then
  DF=$(echo "scale=2; $TOTAL_DEPLOYMENTS / $WEEKS" | bc)
else
  DF="$TOTAL_DEPLOYMENTS"
fi
echo "  Deployment Frequency:  ${DF} deploys/week (${TOTAL_DEPLOYMENTS} total)"

# 2. Lead Time for Changes (median)
if [ -n "$ALL_LEAD_TIMES" ]; then
  MEDIAN_LT=$(echo "$ALL_LEAD_TIMES" | tr ' ' '\n' | sort -n | awk '{a[NR]=$1} END{print a[int(NR/2)+1]}')
  LT_MIN=$(echo "scale=1; $MEDIAN_LT / 60" | bc)
  echo "  Lead Time (median):    ${LT_MIN} minutes (${MEDIAN_LT}s)"
else
  echo "  Lead Time (median):    N/A (no successful pipelines)"
fi

# 3. Change Failure Rate
if [ "$TOTAL_PIPELINES" -gt 0 ]; then
  CFR=$(echo "scale=1; $TOTAL_FAILURES * 100 / $TOTAL_PIPELINES" | bc)
  echo "  Change Failure Rate:   ${CFR}% (${TOTAL_FAILURES}/${TOTAL_PIPELINES})"
else
  echo "  Change Failure Rate:   N/A (no pipelines)"
fi

# 4. Time to Restore (median)
if [ -n "$ALL_RESTORE_TIMES" ]; then
  MEDIAN_RT=$(echo "$ALL_RESTORE_TIMES" | tr ' ' '\n' | sort -n | awk '{a[NR]=$1} END{print a[int(NR/2)+1]}')
  RT_MIN=$(echo "scale=1; $MEDIAN_RT / 60" | bc)
  echo "  Time to Restore:       ${RT_MIN} minutes (${MEDIAN_RT}s)"
else
  echo "  Time to Restore:       N/A (no failure→recovery cycles)"
fi

echo
echo "  DORA Rating Guide:"
echo "    Elite:  DF>1/day, LT<1h, CFR<5%, TTR<1h"
echo "    High:   DF>1/week, LT<1d, CFR<10%, TTR<1d"
echo "    Medium: DF>1/month, LT<1w, CFR<15%, TTR<1w"
echo "    Low:    DF<1/month, LT>1m, CFR>15%, TTR>1m"
echo "------------------------------------------------------------"
