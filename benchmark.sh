#!/bin/bash

set -e

# Generate a random port between 16379 and 26379
REDIS_PORT=$((16379 + RANDOM % 10000))

# Pin both the server and the benchmark client to the same small CPU set, to
# approximate a modestly-sized cloud instance (e.g. an AWS "large" tier)
NUM_CORES=$(nproc)
SERVER_CORE_COUNT=${SERVER_CORE_COUNT:-4}
if [[ $SERVER_CORE_COUNT -gt $NUM_CORES ]]; then
    echo "Error: SERVER_CORE_COUNT ($SERVER_CORE_COUNT) exceeds available cores (nproc=$NUM_CORES)" >&2
    exit 1
fi
SERVER_CORES="0-$((SERVER_CORE_COUNT - 1))"
BENCH_REQUESTS=2000000

# Each benchmark run gets a freshly started server
start_server() {
    taskset -c "$SERVER_CORES" valkey-server --port "$REDIS_PORT" --loadmodule ./target/release/libredisjs.so >/dev/null 2>&1 &
    REDIS_PID=$!

    for _ in $(seq 1 50); do
        if valkey-cli -p "$REDIS_PORT" ping >/dev/null 2>&1; then
            return
        fi
        if ! kill -0 $REDIS_PID 2>/dev/null; then
            break
        fi
        sleep 0.1
    done

    echo "Error: Redis server failed to start" >&2
    exit 1
}

stop_server() {
    if [[ -n $REDIS_PID ]]; then
        kill $REDIS_PID 2>/dev/null || true
        wait $REDIS_PID 2>/dev/null || true
        REDIS_PID=""
    fi
}

run_bench() {
    start_server
    taskset -c "$SERVER_CORES" valkey-benchmark -p "$REDIS_PORT" -n "$BENCH_REQUESTS" "$@"
    stop_server
}

cleanup() {
    echo "Cleaning up..."
    stop_server
    pkill valkey-server 2>/dev/null || true
}

trap cleanup EXIT INT TERM

echo "Building module..."
cargo build --release >/dev/null 2>&1

pkill valkey-server 2>/dev/null || true

echo "Running benchmarks (server and client pinned to cores $SERVER_CORES)..."

echo "=== Simple Math Test ==="
EVALJS_OUTPUT=$(run_bench EVALJS "return 1 + 2" 0 2>/dev/null)
EVALJS_SUMMARY=$(echo "$EVALJS_OUTPUT" | tail -5)

EVAL_OUTPUT=$(run_bench EVAL "return 1 + 2" 0 2>/dev/null)
EVAL_SUMMARY=$(echo "$EVAL_OUTPUT" | tail -5)

echo
echo "EVALJS Results (Math):"
echo "$EVALJS_SUMMARY"
echo
echo "EVAL Results (Math):"
echo "$EVAL_SUMMARY"
echo

EVALJS_MATH_RPS=$(echo "$EVALJS_SUMMARY" | grep "throughput summary" | awk '{print $3}')
EVAL_MATH_RPS=$(echo "$EVAL_SUMMARY" | grep "throughput summary" | awk '{print $3}')

if [[ -n "$EVALJS_MATH_RPS" && -n "$EVAL_MATH_RPS" ]]; then
    MATH_RATIO=$(echo "scale=2; $EVALJS_MATH_RPS / $EVAL_MATH_RPS * 100" | bc)
    echo "Math Test: EVALJS is ${MATH_RATIO}% of EVAL throughput"
fi

echo
echo "=== Redis Call Test ==="
EVALJS_CALL_OUTPUT=$(run_bench EVALJS "return redis.call('SET', 'a', 42)" 0 2>/dev/null)
EVALJS_CALL_SUMMARY=$(echo "$EVALJS_CALL_OUTPUT" | tail -5)

EVAL_CALL_OUTPUT=$(run_bench EVAL "return redis.call('SET', 'a', 42)" 0 2>/dev/null)
EVAL_CALL_SUMMARY=$(echo "$EVAL_CALL_OUTPUT" | tail -5)

echo
echo "EVALJS Results (Redis Call):"
echo "$EVALJS_CALL_SUMMARY"
echo
echo "EVAL Results (Redis Call):"
echo "$EVAL_CALL_SUMMARY"
echo

EVALJS_CALL_RPS=$(echo "$EVALJS_CALL_SUMMARY" | grep "throughput summary" | awk '{print $3}')
EVAL_CALL_RPS=$(echo "$EVAL_CALL_SUMMARY" | grep "throughput summary" | awk '{print $3}')

if [[ -n "$EVALJS_CALL_RPS" && -n "$EVAL_CALL_RPS" ]]; then
    CALL_RATIO=$(echo "scale=2; $EVALJS_CALL_RPS / $EVAL_CALL_RPS * 100" | bc)
    echo "Redis Call Test: EVALJS is ${CALL_RATIO}% of EVAL throughput"
fi

echo
echo "=== Are we there yet? ==="
if [[ -n "$EVALJS_MATH_RPS" && -n "$EVAL_MATH_RPS" ]]; then
    echo "Math: EVALJS ${EVALJS_MATH_RPS} req/s vs EVAL ${EVAL_MATH_RPS} req/s (${MATH_RATIO}%)"
fi
if [[ -n "$EVALJS_CALL_RPS" && -n "$EVAL_CALL_RPS" ]]; then
    echo "Redis Call: EVALJS ${EVALJS_CALL_RPS} req/s vs EVAL ${EVAL_CALL_RPS} req/s (${CALL_RATIO}%)"
fi
