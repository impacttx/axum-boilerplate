#!/usr/bin/env bash
# ==============================================================================
# Performance Benchmark Suite for Axum Backend
# Supports `hey` (preferred for percentiles) and `wrk` (preferred for max rps)
# ==============================================================================
set -euo pipefail

BASE_URL="${BASE_URL:-http://localhost:8000}"
CONCURRENCY="${CONCURRENCY:-50}"
REQUESTS="${REQUESTS:-10000}"
DURATION="${DURATION:-15s}"
THREADS="${THREADS:-4}"

echo "======================================================================"
echo " Starting Benchmark Suite against: ${BASE_URL}"
echo " Concurrency: ${CONCURRENCY} | Requests: ${REQUESTS} | Duration: ${DURATION}"
echo "======================================================================"

# Check for benchmark runners
TOOL=""
if command -v hey >/dev/null 2>&1; then
    TOOL="hey"
elif command -v wrk >/dev/null 2>&1; then
    TOOL="wrk"
else
    echo "⚠️  Neither 'hey' nor 'wrk' was found on your system."
    echo "   Install either tool to run load benchmarks:"
    echo "     macOS: brew install hey (or brew install wrk)"
    echo "     Linux: sudo apt-get install wrk"
    echo "     Go:    go install github.com/rakyll/hey@latest"
    echo ""
    echo "Running quick smoke test using curl..."
    curl -s -o /dev/null -w "Health check response code: %{http_code} (time: %{time_total}s)\n" "${BASE_URL}/health"
    exit 0
fi

echo "Using benchmark tool: ${TOOL}"

# Scenario 1: Raw HTTP Engine Throughput (Healthcheck)
echo ""
echo "----------------------------------------------------------------------"
echo " [Scenario 1/3] Raw HTTP Throughput: GET /health"
echo " (Measures Hyper HTTP/Tokio work-stealing runtime overhead)"
echo "----------------------------------------------------------------------"
if [ "$TOOL" = "hey" ]; then
    hey -n "${REQUESTS}" -c "${CONCURRENCY}" "${BASE_URL}/health"
else
    wrk -t"${THREADS}" -c"${CONCURRENCY}" -d"${DURATION}" "${BASE_URL}/health"
fi

# Scenario 2: DB Read Query Throughput (Items List)
echo ""
echo "----------------------------------------------------------------------"
echo " [Scenario 2/3] DB Read Throughput: GET /v1/items/?limit=20"
echo " (Measures SeaORM + SQLx connection pool + JSON serialization)"
echo "----------------------------------------------------------------------"
if [ "$TOOL" = "hey" ]; then
    hey -n "${REQUESTS}" -c "${CONCURRENCY}" "${BASE_URL}/v1/items/?limit=20"
else
    wrk -t"${THREADS}" -c"${CONCURRENCY}" -d"${DURATION}" "${BASE_URL}/v1/items/?limit=20"
fi

# Scenario 3: DB Write / Transaction Throughput (Item Insert)
echo ""
echo "----------------------------------------------------------------------"
echo " [Scenario 3/3] DB Write Throughput: POST /v1/items/"
echo " (Measures Serde deserialization + transactional insert into PostgreSQL)"
echo "----------------------------------------------------------------------"
PAYLOAD='{"title":"Benchmark Item","description":"Created during load testing","is_completed":false}'
if [ "$TOOL" = "hey" ]; then
    hey -n "$((REQUESTS / 2))" -c "${CONCURRENCY}" -m POST -H "Content-Type: application/json" -d "${PAYLOAD}" "${BASE_URL}/v1/items/"
else
    wrk -t"${THREADS}" -c"${CONCURRENCY}" -d"${DURATION}" -s - "${BASE_URL}/v1/items/" <<EOF
wrk.method = "POST"
wrk.body   = '{"title":"Benchmark Item","description":"Created during load testing","is_completed":false}'
wrk.headers["Content-Type"] = "application/json"
EOF
fi

echo ""
echo "======================================================================"
echo " Benchmark suite completed successfully!"
echo "======================================================================"
